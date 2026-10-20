// Diagnostic only: a per-user broker returns a GUI-owned anonymous endpoint.
#import <Foundation/Foundation.h>
#import <ServiceManagement/ServiceManagement.h>
#include <Security/Security.h>
#include <Security/SecTask.h>
#include <dispatch/dispatch.h>
#include <xpc/xpc.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static const char *gui_id = "app.quickpresenter.xpc-probe.gui";
static const char *broker_id = "app.quickpresenter.xpc-probe.broker";
static const char *client_id = "app.quickpresenter.xpc-probe.client";
static char register_name[192];
static char lookup_name[192];
static xpc_object_t registered_endpoint;
static xpc_connection_t registration_owner;

static void json_error(const char *stage, int value) {
    fprintf(stderr, "{\"probe\":1,\"error\":\"%s\",\"value\":%d}\n", stage, value);
}

static bool signing_group(void) {
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
    SecTaskRef task = SecTaskCreateFromSelf(NULL);
    CFTypeRef sandbox = task ? SecTaskCopyValueForEntitlement(task,
        CFSTR("com.apple.security.app-sandbox"), NULL) : NULL;
    CFTypeRef groups = task ? SecTaskCopyValueForEntitlement(task,
        CFSTR("com.apple.security.application-groups"), NULL) : NULL;
    char group[128];
    snprintf(group, sizeof(group), "%s.app.quickpresenter.control-xpc-probe", team_text);
    CFStringRef expected = CFStringCreateWithCString(NULL, group, kCFStringEncodingUTF8);
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

static int set_identity(xpc_connection_t connection, const char *identifier) {
    if (__builtin_available(macOS 14.4, *)) {
        return xpc_connection_set_peer_team_identity_requirement(connection, identifier);
    }
    return -1;
}

static xpc_object_t request(xpc_connection_t connection, xpc_object_t message) {
    xpc_object_t result = xpc_connection_send_message_with_reply_sync(connection, message);
    if (xpc_get_type(result) == XPC_TYPE_ERROR) {
        const char *description = xpc_dictionary_get_string(result, XPC_ERROR_KEY_DESCRIPTION);
        fprintf(stderr, "{\"probe\":1,\"xpc_error\":\"%s\"}\n",
                description ? description : "unknown");
    }
    return result;
}

static xpc_connection_t named_peer(const char *name, const char *identifier) {
    xpc_connection_t connection = xpc_connection_create_mach_service(name, NULL, 0);
    if (!connection || set_identity(connection, identifier) != 0) return NULL;
    xpc_connection_set_event_handler(connection, ^(xpc_object_t event) { (void)event; });
    xpc_connection_activate(connection);
    return connection;
}

static void reply_status(xpc_object_t message, bool accepted) {
    xpc_object_t reply = xpc_dictionary_create_reply(message);
    if (!reply) return;
    xpc_dictionary_set_bool(reply, "accepted", accepted);
    xpc_connection_t remote = xpc_dictionary_get_remote_connection(message);
    xpc_connection_send_message(remote, reply);
    xpc_release(reply);
}

static void accept_registration(xpc_connection_t peer) {
    if (set_identity(peer, gui_id) != 0) { xpc_connection_cancel(peer); return; }
    xpc_connection_set_target_queue(peer, dispatch_get_main_queue());
    xpc_connection_set_event_handler(peer, ^(xpc_object_t message) {
        if (xpc_get_type(message) == XPC_TYPE_ERROR) {
            if (registration_owner == peer) {
                if (registered_endpoint) xpc_release(registered_endpoint);
                registered_endpoint = NULL;
                registration_owner = NULL;
            }
            xpc_connection_cancel(peer);
            return;
        }
        xpc_object_t endpoint = xpc_dictionary_get_value(message, "endpoint");
        bool accepted = xpc_get_type(message) == XPC_TYPE_DICTIONARY &&
            endpoint && xpc_get_type(endpoint) == XPC_TYPE_ENDPOINT &&
            !registered_endpoint && strcmp(xpc_dictionary_get_string(message, "kind") ?: "", "register") == 0;
        if (accepted) {
            registered_endpoint = xpc_retain(endpoint);
            registration_owner = peer;
        }
        reply_status(message, accepted);
    });
    xpc_connection_activate(peer);
}

static void accept_lookup(xpc_connection_t peer) {
    if (set_identity(peer, client_id) != 0) { xpc_connection_cancel(peer); return; }
    xpc_connection_set_target_queue(peer, dispatch_get_main_queue());
    xpc_connection_set_event_handler(peer, ^(xpc_object_t message) {
        if (xpc_get_type(message) == XPC_TYPE_ERROR) { xpc_connection_cancel(peer); return; }
        xpc_object_t reply = xpc_dictionary_create_reply(message);
        if (!reply) return;
        bool accepted = xpc_get_type(message) == XPC_TYPE_DICTIONARY &&
            strcmp(xpc_dictionary_get_string(message, "kind") ?: "", "lookup") == 0 &&
            registered_endpoint;
        xpc_dictionary_set_bool(reply, "accepted", accepted);
        if (accepted) xpc_dictionary_set_value(reply, "endpoint", registered_endpoint);
        xpc_connection_send_message(peer, reply);
        xpc_release(reply);
    });
    xpc_connection_activate(peer);
}

static int run_broker(void) {
    xpc_connection_t registration = xpc_connection_create_mach_service(register_name,
        dispatch_get_main_queue(), XPC_CONNECTION_MACH_SERVICE_LISTENER);
    xpc_connection_t lookup = xpc_connection_create_mach_service(lookup_name,
        dispatch_get_main_queue(), XPC_CONNECTION_MACH_SERVICE_LISTENER);
    if (!registration || !lookup) { json_error("listeners", 0); return 1; }
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

static void accept_client(xpc_connection_t peer) {
    if (set_identity(peer, client_id) != 0) { xpc_connection_cancel(peer); return; }
    xpc_connection_set_event_handler(peer, ^(xpc_object_t message) {
        if (xpc_get_type(message) == XPC_TYPE_ERROR) { xpc_connection_cancel(peer); return; }
        xpc_object_t reply = xpc_dictionary_create_reply(message);
        if (!reply) return;
        bool accepted = xpc_get_type(message) == XPC_TYPE_DICTIONARY &&
            strcmp(xpc_dictionary_get_string(message, "kind") ?: "", "ping") == 0;
        xpc_dictionary_set_bool(reply, "accepted", accepted);
        xpc_connection_send_message(peer, reply);
        xpc_release(reply);
    });
    xpc_connection_activate(peer);
}

static int run_gui(unsigned seconds) {
    xpc_connection_t listener = xpc_connection_create(NULL, dispatch_get_main_queue());
    if (!listener) { json_error("anonymous_listener", 0); return 1; }
    xpc_connection_set_event_handler(listener, ^(xpc_object_t peer) {
        if (xpc_get_type(peer) == XPC_TYPE_CONNECTION) accept_client(peer);
    });
    xpc_connection_activate(listener);
    xpc_object_t endpoint = xpc_endpoint_create(listener);
    xpc_connection_t broker = named_peer(register_name, broker_id);
    if (!endpoint || !broker) { json_error("broker_connection", 0); return 1; }
    xpc_object_t message = xpc_dictionary_create(NULL, NULL, 0);
    xpc_dictionary_set_string(message, "kind", "register");
    xpc_dictionary_set_value(message, "endpoint", endpoint);
    xpc_object_t response = request(broker, message);
    bool accepted = xpc_get_type(response) == XPC_TYPE_DICTIONARY &&
        xpc_dictionary_get_bool(response, "accepted");
    xpc_release(response); xpc_release(message); xpc_release(endpoint);
    if (!accepted) { json_error("registration_rejected", 0); return 7; }
    printf("{\"probe\":1,\"event\":\"gui_registered\"}\n"); fflush(stdout);
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)seconds * NSEC_PER_SEC),
        dispatch_get_main_queue(), ^{ exit(0); });
    dispatch_main();
}

static int run_client(void) {
    xpc_connection_t broker = named_peer(lookup_name, broker_id);
    if (!broker) { json_error("broker_connection", 0); return 7; }
    xpc_object_t message = xpc_dictionary_create(NULL, NULL, 0);
    xpc_dictionary_set_string(message, "kind", "lookup");
    xpc_object_t response = request(broker, message);
    bool accepted = xpc_get_type(response) == XPC_TYPE_DICTIONARY &&
        xpc_dictionary_get_bool(response, "accepted");
    xpc_object_t endpoint = accepted ? xpc_dictionary_get_value(response, "endpoint") : NULL;
    if (!endpoint || xpc_get_type(endpoint) != XPC_TYPE_ENDPOINT) {
        json_error("lookup_rejected", 0);
        xpc_release(response); xpc_release(message); return 7;
    }
    xpc_connection_t gui = xpc_connection_create_from_endpoint(endpoint);
    if (!gui || set_identity(gui, gui_id) != 0) {
        json_error("gui_connection", 0); return 7;
    }
    xpc_connection_set_event_handler(gui, ^(xpc_object_t event) { (void)event; });
    xpc_connection_activate(gui);
    xpc_object_t ping = xpc_dictionary_create(NULL, NULL, 0);
    xpc_dictionary_set_string(ping, "kind", "ping");
    xpc_object_t pong = request(gui, ping);
    bool success = xpc_get_type(pong) == XPC_TYPE_DICTIONARY &&
        xpc_dictionary_get_bool(pong, "accepted");
    if (success) printf("{\"probe\":1,\"event\":\"roundtrip\"}\n");
    else json_error("direct_rejected", 0);
    xpc_release(pong); xpc_release(ping); xpc_release(response); xpc_release(message);
    return success ? 0 : 7;
}

static int service(const char *action) {
    NSString *plist = @"app.quickpresenter.xpc-probe.agent.plist";
    SMAppService *item = [SMAppService agentServiceWithPlistName:plist];
    NSError *error = nil;
    if (strcmp(action, "register") == 0) {
        if (![item registerAndReturnError:&error]) {
            fprintf(stderr, "{\"probe\":1,\"error\":\"register\",\"code\":%ld}\n",
                    (long)error.code);
            return 7;
        }
    } else if (strcmp(action, "unregister") == 0) {
        if (![item unregisterAndReturnError:&error]) {
            fprintf(stderr, "{\"probe\":1,\"error\":\"unregister\",\"code\":%ld}\n",
                    (long)error.code);
            return 7;
        }
    }
    printf("{\"probe\":1,\"status\":%ld}\n", (long)item.status);
    return 0;
}

int main(int argc, char **argv) {
    @autoreleasepool {
        if (!signing_group()) { json_error("self_identity", 0); return 7; }
        if (argc == 2 && strcmp(argv[1], "broker") == 0) return run_broker();
        if (argc == 2 && strcmp(argv[1], "client") == 0) return run_client();
        if (argc == 3 && strcmp(argv[1], "gui") == 0) {
            char *end = NULL;
            unsigned long seconds = strtoul(argv[2], &end, 10);
            if (!end || *end || seconds == 0 || seconds > 60) return 2;
            return run_gui((unsigned)seconds);
        }
        if (argc == 2 && (strcmp(argv[1], "register") == 0 ||
                          strcmp(argv[1], "unregister") == 0 ||
                          strcmp(argv[1], "status") == 0)) return service(argv[1]);
        json_error("usage", argc); return 2;
    }
}
