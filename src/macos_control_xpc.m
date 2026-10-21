// Product Store control transport. XPC authenticates each message; Rust owns
// protocol parsing, presentation state, and response/watch limits.
#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <ServiceManagement/ServiceManagement.h>
#include <Security/Security.h>
#include <Security/SecTask.h>
#include <dispatch/dispatch.h>
#include <xpc/xpc.h>
#include <errno.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

static const char *const gui_id = "app.quickpresenter.QuickPresenter";
static const char *const broker_id = "app.quickpresenter.control.broker";
static const char *const cli_id = "app.quickpresenter.qp";
static const char *const agent_plist = "app.quickpresenter.control.agent.plist";

int qp_control_launch_gui(const char *app_path) {
    if (!app_path) return EINVAL;
    @autoreleasepool {
        NSURL *url = [NSURL fileURLWithFileSystemRepresentation:app_path isDirectory:YES
                                                  relativeToURL:nil];
        [[NSWorkspace sharedWorkspace] openApplicationAtURL:url
                                               configuration:[NSWorkspaceOpenConfiguration configuration]
                                           completionHandler:nil];
        return 0;
    }
}

void qp_control_cancel_cli_open_panel(void) {
    for (NSWindow *window in NSApp.windows) {
        NSWindow *sheet = window.attachedSheet;
        if ([sheet isKindOfClass:[NSOpenPanel class]]) {
            [(NSOpenPanel *)sheet cancel:nil];
            return;
        }
    }
    NSWindow *key = NSApp.keyWindow;
    if ([key isKindOfClass:[NSOpenPanel class]])
        [(NSOpenPanel *)key cancel:nil];
}
static char register_name[160];
static char lookup_name[160];

typedef void *(*request_callback_t)(void *, const void *, size_t, void *);
typedef void (*cancel_callback_t)(void *);
static request_callback_t gui_request;
static cancel_callback_t gui_cancel;
static void *gui_context;

static bool signing_group(const char *identifier) {
    SecCodeRef self = NULL;
    CFDictionaryRef info = NULL;
    bool valid = SecCodeCopySelf(kSecCSDefaultFlags, &self) == errSecSuccess &&
        SecCodeCheckValidity(self, kSecCSDefaultFlags, NULL) == errSecSuccess &&
        SecCodeCopySigningInformation(self, kSecCSSigningInformation, &info) == errSecSuccess;
    CFStringRef team = valid ? CFDictionaryGetValue(info, kSecCodeInfoTeamIdentifier) : NULL;
    char team_text[64] = {0};
    valid = valid && team && CFGetTypeID(team) == CFStringGetTypeID() &&
        CFStringGetCString(team, team_text, sizeof(team_text), kCFStringEncodingUTF8) &&
        strlen(team_text) == 10;
    CFStringRef signed_identifier = valid ? CFDictionaryGetValue(info, kSecCodeInfoIdentifier) : NULL;
    CFStringRef expected_identifier = CFStringCreateWithCString(NULL, identifier,
        kCFStringEncodingUTF8);
    valid = valid && signed_identifier && expected_identifier &&
        CFEqual(signed_identifier, expected_identifier);
    if (expected_identifier) CFRelease(expected_identifier);
    SecTaskRef task = SecTaskCreateFromSelf(NULL);
    CFTypeRef sandbox = task ? SecTaskCopyValueForEntitlement(task,
        CFSTR("com.apple.security.app-sandbox"), NULL) : NULL;
    CFTypeRef groups = task ? SecTaskCopyValueForEntitlement(task,
        CFSTR("com.apple.security.application-groups"), NULL) : NULL;
    char group[128] = {0};
    if (valid) snprintf(group, sizeof(group), "%s.app.quickpresenter.control", team_text);
    CFStringRef expected = valid ? CFStringCreateWithCString(NULL, group,
        kCFStringEncodingUTF8) : NULL;
    valid = valid && sandbox && CFEqual(sandbox, kCFBooleanTrue) && groups && expected &&
        CFGetTypeID(groups) == CFArrayGetTypeID() && CFArrayGetCount((CFArrayRef)groups) == 1 &&
        CFEqual(CFArrayGetValueAtIndex((CFArrayRef)groups, 0), expected);
    if (expected) CFRelease(expected);
    if (groups) CFRelease(groups);
    if (sandbox) CFRelease(sandbox);
    if (task) CFRelease(task);
    if (info) CFRelease(info);
    if (self) CFRelease(self);
    if (!valid) return false;
    snprintf(register_name, sizeof(register_name), "%s.register", group);
    snprintf(lookup_name, sizeof(lookup_name), "%s.lookup", group);
    return true;
}

static bool require_peer(xpc_connection_t peer, const char *identifier) {
    if (__builtin_available(macOS 14.4, *)) {
        return xpc_connection_set_peer_team_identity_requirement(peer, identifier) == 0;
    }
    return false;
}

typedef struct {
    atomic_int references;
    pthread_mutex_t mutex;
    dispatch_semaphore_t semaphore;
    xpc_object_t result;
} ReplyWait;

static void release_wait(ReplyWait *wait) {
    if (atomic_fetch_sub(&wait->references, 1) == 1) {
        if (wait->result) xpc_release(wait->result);
        dispatch_release(wait->semaphore);
        pthread_mutex_destroy(&wait->mutex);
        free(wait);
    }
}

static xpc_object_t request_with_deadline(
    xpc_connection_t peer, xpc_object_t message, int64_t timeout_ns) {
    ReplyWait *wait = calloc(1, sizeof(*wait));
    if (!wait) return NULL;
    atomic_init(&wait->references, 2);
    pthread_mutex_init(&wait->mutex, NULL);
    wait->semaphore = dispatch_semaphore_create(0);
    xpc_connection_send_message_with_reply(peer, message,
        dispatch_get_global_queue(QOS_CLASS_DEFAULT, 0), ^(xpc_object_t response) {
        pthread_mutex_lock(&wait->mutex);
        wait->result = xpc_retain(response);
        pthread_mutex_unlock(&wait->mutex);
        dispatch_semaphore_signal(wait->semaphore);
        release_wait(wait);
    });
    xpc_object_t result = NULL;
    if (dispatch_semaphore_wait(wait->semaphore,
        dispatch_time(DISPATCH_TIME_NOW, timeout_ns)) == 0) {
        pthread_mutex_lock(&wait->mutex);
        if (wait->result) result = xpc_retain(wait->result);
        pthread_mutex_unlock(&wait->mutex);
    }
    release_wait(wait);
    return result;
}

static void cancel_inactive(xpc_connection_t connection) {
    xpc_connection_set_event_handler(connection, ^(xpc_object_t event) { (void)event; });
    xpc_connection_activate(connection);
    xpc_connection_cancel(connection);
}

static void discard_inactive(xpc_connection_t connection) {
    cancel_inactive(connection);
    xpc_release(connection);
}

static xpc_connection_t named_peer(const char *name, const char *identifier) {
    xpc_connection_t peer = xpc_connection_create_mach_service(name, NULL, 0);
    if (!peer) return NULL;
    if (!require_peer(peer, identifier)) { discard_inactive(peer); return NULL; }
    xpc_connection_set_event_handler(peer, ^(xpc_object_t event) { (void)event; });
    xpc_connection_activate(peer);
    return peer;
}

static void send_acceptance(xpc_object_t message, bool accepted, xpc_object_t endpoint) {
    xpc_object_t reply = xpc_dictionary_create_reply(message);
    if (!reply) return;
    xpc_dictionary_set_bool(reply, "accepted", accepted);
    if (accepted && endpoint) xpc_dictionary_set_value(reply, "endpoint", endpoint);
    xpc_connection_t remote = xpc_dictionary_get_remote_connection(message);
    xpc_connection_send_message(remote, reply);
    xpc_release(reply);
}

static xpc_object_t current_endpoint;
static xpc_connection_t endpoint_owner;
typedef struct { xpc_connection_t peer; } BrokerPeer;

static void free_broker_peer(void *context) { free(context); }

static BrokerPeer *broker_peer(xpc_connection_t peer) {
    BrokerPeer *state = calloc(1, sizeof(*state));
    if (!state) { cancel_inactive(peer); return NULL; }
    state->peer = peer;
    xpc_connection_set_target_queue(peer, dispatch_get_main_queue());
    xpc_connection_set_context(peer, state);
    xpc_connection_set_finalizer_f(peer, free_broker_peer);
    return state;
}

static void accept_registration(xpc_connection_t peer) {
    if (xpc_connection_get_euid(peer) != geteuid() || !require_peer(peer, gui_id)) {
        cancel_inactive(peer); return;
    }
    BrokerPeer *state = broker_peer(peer);
    if (!state) return;
    xpc_connection_set_event_handler(peer, ^(xpc_object_t message) {
        if (xpc_get_type(message) == XPC_TYPE_ERROR) {
            if (endpoint_owner == state->peer) {
                if (current_endpoint) xpc_release(current_endpoint);
                current_endpoint = NULL;
                endpoint_owner = NULL;
            }
            xpc_connection_cancel(state->peer);
            return;
        }
        xpc_object_t endpoint = xpc_dictionary_get_value(message, "endpoint");
        bool accepted = xpc_get_type(message) == XPC_TYPE_DICTIONARY &&
            strcmp(xpc_dictionary_get_string(message, "kind") ?: "", "register") == 0 &&
            endpoint && xpc_get_type(endpoint) == XPC_TYPE_ENDPOINT && !current_endpoint;
        if (accepted) {
            current_endpoint = xpc_retain(endpoint);
            endpoint_owner = state->peer;
        }
        send_acceptance(message, accepted, NULL);
    });
    xpc_connection_activate(peer);
}

static void accept_lookup(xpc_connection_t peer) {
    if (xpc_connection_get_euid(peer) != geteuid() || !require_peer(peer, cli_id)) {
        cancel_inactive(peer); return;
    }
    BrokerPeer *state = broker_peer(peer);
    if (!state) return;
    xpc_connection_set_event_handler(peer, ^(xpc_object_t message) {
        if (xpc_get_type(message) == XPC_TYPE_ERROR) {
            xpc_connection_cancel(state->peer); return;
        }
        bool accepted = xpc_get_type(message) == XPC_TYPE_DICTIONARY &&
            strcmp(xpc_dictionary_get_string(message, "kind") ?: "", "lookup") == 0 &&
            current_endpoint;
        send_acceptance(message, accepted, current_endpoint);
    });
    xpc_connection_activate(peer);
}

int qp_control_broker_main(void) {
    @autoreleasepool {
        if (!signing_group(broker_id)) return 7;
        xpc_connection_t registration = xpc_connection_create_mach_service(register_name,
            dispatch_get_main_queue(), XPC_CONNECTION_MACH_SERVICE_LISTENER);
        xpc_connection_t lookup = xpc_connection_create_mach_service(lookup_name,
            dispatch_get_main_queue(), XPC_CONNECTION_MACH_SERVICE_LISTENER);
        if (!registration || !lookup) {
            if (registration) discard_inactive(registration);
            if (lookup) discard_inactive(lookup);
            return 7;
        }
        xpc_connection_set_event_handler(registration, ^(xpc_object_t peer) {
            if (xpc_get_type(peer) == XPC_TYPE_CONNECTION) accept_registration(peer);
        });
        xpc_connection_set_event_handler(lookup, ^(xpc_object_t peer) {
            if (xpc_get_type(peer) == XPC_TYPE_CONNECTION) accept_lookup(peer);
        });
        xpc_connection_activate(registration);
        xpc_connection_activate(lookup);
        dispatch_main();
    }
}

typedef struct { void *session; xpc_connection_t peer; } ServerPeer;

static void free_server_peer(void *context) { free(context); }

static void accept_client(xpc_connection_t peer) {
    if (xpc_connection_get_euid(peer) != geteuid() || !require_peer(peer, cli_id)) {
        cancel_inactive(peer); return;
    }
    ServerPeer *state = calloc(1, sizeof(*state));
    if (!state) { cancel_inactive(peer); return; }
    state->peer = peer;
    xpc_connection_set_context(peer, state);
    xpc_connection_set_finalizer_f(peer, free_server_peer);
    xpc_connection_set_event_handler(peer, ^(xpc_object_t message) {
        if (xpc_get_type(message) == XPC_TYPE_ERROR) {
            if (state->session) {
                gui_cancel(state->session);
                state->session = NULL;
            }
            xpc_connection_cancel(state->peer);
            return;
        }
        if (xpc_get_type(message) != XPC_TYPE_DICTIONARY) {
            xpc_connection_cancel(state->peer); return;
        }
        const char *kind = xpc_dictionary_get_string(message, "kind");
        if (kind && strcmp(kind, "hello") == 0 && !state->session) {
            send_acceptance(message, true, NULL);
            return;
        }
        size_t length = 0;
        const void *data = xpc_dictionary_get_data(message, "data", &length);
        if (!kind || strcmp(kind, "request") != 0 || state->session || !data ||
            length == 0 || length > 1024 * 1024) {
            xpc_connection_cancel(state->peer); return;
        }
        state->session = gui_request(state->peer, data, length, gui_context);
        if (!state->session) xpc_connection_cancel(state->peer);
    });
    xpc_connection_activate(peer);
}

typedef struct {
    xpc_connection_t listener;
    xpc_connection_t broker;
} GuiHandle;

void *qp_control_gui_start(request_callback_t request, cancel_callback_t cancel, void *context) {
    @autoreleasepool {
        if (!request || !cancel || !signing_group(gui_id)) return NULL;
        SMAppService *service = [SMAppService agentServiceWithPlistName:
            [NSString stringWithUTF8String:agent_plist]];
        if (service.status != SMAppServiceStatusEnabled) {
            NSError *error = nil;
            if (![service registerAndReturnError:&error] ||
                service.status != SMAppServiceStatusEnabled) return NULL;
        }
        GuiHandle *handle = calloc(1, sizeof(*handle));
        if (!handle) return NULL;
        gui_request = request;
        gui_cancel = cancel;
        gui_context = context;
        handle->listener = xpc_connection_create(NULL,
            dispatch_get_global_queue(QOS_CLASS_DEFAULT, 0));
        handle->broker = named_peer(register_name, broker_id);
        if (!handle->listener || !handle->broker) goto failure;
        xpc_connection_set_event_handler(handle->listener, ^(xpc_object_t peer) {
            if (xpc_get_type(peer) == XPC_TYPE_CONNECTION) accept_client(peer);
        });
        xpc_connection_activate(handle->listener);
        xpc_object_t endpoint = xpc_endpoint_create(handle->listener);
        xpc_object_t message = xpc_dictionary_create(NULL, NULL, 0);
        xpc_dictionary_set_string(message, "kind", "register");
        xpc_dictionary_set_value(message, "endpoint", endpoint);
        xpc_object_t response = request_with_deadline(handle->broker, message,
            3 * NSEC_PER_SEC);
        bool accepted = response && xpc_get_type(response) == XPC_TYPE_DICTIONARY &&
            xpc_dictionary_get_bool(response, "accepted");
        if (response) xpc_release(response);
        xpc_release(message);
        xpc_release(endpoint);
        if (!accepted) goto failure;
        return handle;
    failure:
        if (handle->listener) {
            if (handle->broker) {
                xpc_connection_cancel(handle->listener);
                xpc_release(handle->listener);
            } else {
                discard_inactive(handle->listener);
            }
        }
        if (handle->broker) { xpc_connection_cancel(handle->broker); xpc_release(handle->broker); }
        free(handle);
        return NULL;
    }
}

void qp_control_gui_stop(void *opaque) {
    GuiHandle *handle = opaque;
    if (!handle) return;
    xpc_connection_cancel(handle->listener);
    xpc_connection_cancel(handle->broker);
    xpc_release(handle->listener);
    xpc_release(handle->broker);
    // One GUI registration per process. The listener callback context stays
    // valid through asynchronous XPC cancellation until process exit.
}

int qp_control_peer_send(void *opaque, const void *bytes, size_t length) {
    if (!opaque || !bytes || length == 0 || length > 1024 * 1024) return -EINVAL;
    xpc_object_t message = xpc_dictionary_create(NULL, NULL, 0);
    xpc_dictionary_set_string(message, "kind", "response");
    xpc_dictionary_set_data(message, "data", bytes, length);
    xpc_object_t acknowledgement = request_with_deadline((xpc_connection_t)opaque, message,
        NSEC_PER_SEC);
    bool accepted = acknowledgement && xpc_get_type(acknowledgement) == XPC_TYPE_DICTIONARY &&
        xpc_dictionary_get_bool(acknowledgement, "accepted");
    if (acknowledgement) xpc_release(acknowledgement);
    xpc_release(message);
    return accepted ? 0 : -ETIMEDOUT;
}

void qp_control_peer_retain(void *opaque) { xpc_retain((xpc_connection_t)opaque); }
void qp_control_peer_release(void *opaque) { xpc_release((xpc_connection_t)opaque); }
void qp_control_peer_cancel(void *opaque) { xpc_connection_cancel((xpc_connection_t)opaque); }

#define CLIENT_QUEUE_CAPACITY 64
#define CLIENT_QUEUE_BYTES (34 * 1024 * 1024)
typedef struct {
    xpc_connection_t peer;
    pthread_mutex_t mutex;
    pthread_cond_t changed;
    xpc_object_t queue[CLIENT_QUEUE_CAPACITY];
    size_t head, count, bytes;
    bool closed;
} ClientHandle;

static void free_client(void *opaque) {
    ClientHandle *client = opaque;
    for (size_t i = 0; i < client->count; i++)
        xpc_release(client->queue[(client->head + i) % CLIENT_QUEUE_CAPACITY]);
    pthread_cond_destroy(&client->changed);
    pthread_mutex_destroy(&client->mutex);
    free(client);
}

void *qp_control_client_connect(int *error_out) {
    if (error_out) *error_out = EACCES;
    if (!signing_group(cli_id)) { if (error_out) *error_out = EACCES; return NULL; }
    xpc_connection_t broker = named_peer(lookup_name, broker_id);
    if (!broker) { if (error_out) *error_out = EACCES; return NULL; }
    xpc_object_t lookup = xpc_dictionary_create(NULL, NULL, 0);
    xpc_dictionary_set_string(lookup, "kind", "lookup");
    xpc_object_t response = request_with_deadline(broker, lookup, 3 * NSEC_PER_SEC);
    if (!response && error_out) *error_out = ETIMEDOUT;
    if (response && xpc_equal(response, XPC_ERROR_PEER_CODE_SIGNING_REQUIREMENT) && error_out)
        *error_out = EACCES;
    if (response && xpc_equal(response, XPC_ERROR_CONNECTION_INVALID) && error_out)
        *error_out = ENOENT;
    if (response && xpc_get_type(response) == XPC_TYPE_DICTIONARY &&
        !xpc_dictionary_get_bool(response, "accepted") && error_out)
        *error_out = ENOENT;
    xpc_object_t endpoint = response && xpc_get_type(response) == XPC_TYPE_DICTIONARY &&
        xpc_dictionary_get_bool(response, "accepted") ?
        xpc_dictionary_get_value(response, "endpoint") : NULL;
    xpc_connection_t peer = endpoint && xpc_get_type(endpoint) == XPC_TYPE_ENDPOINT ?
        xpc_connection_create_from_endpoint(endpoint) : NULL;
    if (response) xpc_release(response);
    xpc_release(lookup);
    xpc_connection_cancel(broker);
    xpc_release(broker);
    if (!peer) return NULL;
    if (!require_peer(peer, gui_id)) {
        if (error_out) *error_out = EACCES;
        discard_inactive(peer); return NULL;
    }
    ClientHandle *client = calloc(1, sizeof(*client));
    if (!client) { discard_inactive(peer); return NULL; }
    pthread_mutex_init(&client->mutex, NULL);
    pthread_cond_init(&client->changed, NULL);
    client->peer = peer;
    xpc_connection_set_context(peer, client);
    xpc_connection_set_finalizer_f(peer, free_client);
    xpc_connection_set_event_handler(peer, ^(xpc_object_t message) {
        pthread_mutex_lock(&client->mutex);
        if (xpc_get_type(message) == XPC_TYPE_ERROR) {
            client->closed = true;
        } else if (xpc_get_type(message) == XPC_TYPE_DICTIONARY && !client->closed) {
            size_t length = 0;
            const void *data = xpc_dictionary_get_data(message, "data", &length);
            if (data && length && length <= 1024 * 1024 &&
                client->count < CLIENT_QUEUE_CAPACITY &&
                client->bytes + length <= CLIENT_QUEUE_BYTES) {
                client->queue[(client->head + client->count) % CLIENT_QUEUE_CAPACITY] =
                    xpc_retain(message);
                client->count++;
                client->bytes += length;
                send_acceptance(message, true, NULL);
            } else {
                client->closed = true;
                xpc_connection_cancel(client->peer);
            }
        }
        pthread_cond_broadcast(&client->changed);
        pthread_mutex_unlock(&client->mutex);
    });
    xpc_connection_activate(peer);
    xpc_object_t hello = xpc_dictionary_create(NULL, NULL, 0);
    xpc_dictionary_set_string(hello, "kind", "hello");
    xpc_object_t answer = request_with_deadline(peer, hello, 3 * NSEC_PER_SEC);
    if (!answer && error_out) *error_out = ETIMEDOUT;
    if (answer && xpc_equal(answer, XPC_ERROR_PEER_CODE_SIGNING_REQUIREMENT) && error_out)
        *error_out = EACCES;
    if (answer && xpc_equal(answer, XPC_ERROR_CONNECTION_INVALID) && error_out)
        *error_out = ENOENT;
    bool accepted = answer && xpc_get_type(answer) == XPC_TYPE_DICTIONARY &&
        xpc_dictionary_get_bool(answer, "accepted");
    if (answer) xpc_release(answer);
    xpc_release(hello);
    if (!accepted) { xpc_connection_cancel(peer); xpc_release(peer); return NULL; }
    return client;
}

int qp_control_client_send(void *opaque, const void *bytes, size_t length) {
    ClientHandle *client = opaque;
    if (!client || !bytes || !length || length > 1024 * 1024) return -EINVAL;
    pthread_mutex_lock(&client->mutex);
    bool closed = client->closed;
    pthread_mutex_unlock(&client->mutex);
    if (closed) return -EPIPE;
    xpc_object_t message = xpc_dictionary_create(NULL, NULL, 0);
    xpc_dictionary_set_string(message, "kind", "request");
    xpc_dictionary_set_data(message, "data", bytes, length);
    xpc_connection_send_message(client->peer, message);
    xpc_release(message);
    return 0;
}

ssize_t qp_control_client_next(void *opaque, void *buffer, size_t capacity, unsigned timeout_ms) {
    ClientHandle *client = opaque;
    if (!client || !buffer) return -EINVAL;
    struct timespec deadline;
    clock_gettime(CLOCK_REALTIME, &deadline);
    deadline.tv_sec += timeout_ms / 1000;
    deadline.tv_nsec += (long)(timeout_ms % 1000) * 1000000L;
    if (deadline.tv_nsec >= 1000000000L) { deadline.tv_sec++; deadline.tv_nsec -= 1000000000L; }
    pthread_mutex_lock(&client->mutex);
    while (!client->count && !client->closed) {
        if (pthread_cond_timedwait(&client->changed, &client->mutex, &deadline) == ETIMEDOUT) {
            pthread_mutex_unlock(&client->mutex);
            return -ETIMEDOUT;
        }
    }
    if (!client->count) { pthread_mutex_unlock(&client->mutex); return -EPIPE; }
    xpc_object_t message = client->queue[client->head];
    client->head = (client->head + 1) % CLIENT_QUEUE_CAPACITY;
    client->count--;
    size_t length = 0;
    const void *data = xpc_dictionary_get_data(message, "data", &length);
    client->bytes -= length;
    ssize_t result = length <= capacity ? (ssize_t)length : -EMSGSIZE;
    if (result > 0) memcpy(buffer, data, length);
    xpc_release(message);
    pthread_mutex_unlock(&client->mutex);
    return result;
}

void qp_control_client_close(void *opaque) {
    ClientHandle *client = opaque;
    if (!client) return;
    xpc_connection_cancel(client->peer);
    xpc_release(client->peer);
}
