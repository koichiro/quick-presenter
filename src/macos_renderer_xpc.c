// Supported libXPC bootstrap only. PDF protocol/state remain in Rust.
#include <xpc/xpc.h>
#include <dispatch/dispatch.h>
#include <Security/Security.h>
#include <Security/SecTask.h>
#include <CoreFoundation/CoreFoundation.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <signal.h>
#include <libproc.h>
#include <sys/resource.h>
#include <stdio.h>
#include <os/log.h>
#include <ctype.h>

extern void qp_renderer_run(int document_fd);
static const char *service_name = "org.quickpresenter.renderer";
static atomic_bool accepted;
static atomic_bool disconnected;
static const char *proxy_identifier = "app.quickpresenter.renderer-proxy";
static char signing_team[64];
static const char *probe_keys[] = {
    "QUICK_PRESENTER_SANDBOX_DENIAL_PROBE",
    "QUICK_PRESENTER_SANDBOX_CONNECT_PROBE",
#ifndef NDEBUG
    "QUICK_PRESENTER_HELPER_TEST_FAULT",
    "QUICK_PRESENTER_HELPER_TEST_FAULT_PAGE",
    "QUICK_PRESENTER_HELPER_TEST_FAULT_TITLE",
    "QUICK_PRESENTER_HELPER_TEST_DEADLINE_MS",
#endif
};

// Read our own validated signing identity; never trust a peer-supplied team or
// the identity of a potentially replaced executable at a nested bundle path.
static bool copy_signing_team(SecCodeRef code, char *team, size_t size) {
    CFDictionaryRef info = NULL;
    bool valid = false;
    if (SecCodeCopySigningInformation(code, kSecCSSigningInformation, &info) == errSecSuccess) {
        CFStringRef value = CFDictionaryGetValue(info, kSecCodeInfoTeamIdentifier);
        if (value && CFGetTypeID(value) == CFStringGetTypeID() &&
            CFStringGetCString(value, team, size, kCFStringEncodingUTF8)) {
            valid = strlen(team) > 0;
            for (size_t i = 0; team[i]; i++) if (!isalnum((unsigned char)team[i])) valid = false;
        }
    }
    if (info) CFRelease(info);
    return valid;
}

static bool load_signing_team(void) {
    SecCodeRef self = NULL;
    bool valid = SecCodeCopySelf(kSecCSDefaultFlags, &self) == errSecSuccess &&
        copy_signing_team(self, signing_team, sizeof(signing_team));
    if (self) CFRelease(self);
    return valid;
}

static int set_peer_identity(xpc_connection_t connection, const char *identifier) {
    if (__builtin_available(macOS 14.4, *)) {
        // Handles Apple re-signing for TestFlight and App Store distribution.
        return xpc_connection_set_peer_team_identity_requirement(connection, identifier);
    }
    // Keep the original Developer ID policy on older systems. libXPC checks
    // this requirement on every message in both directions. Apple re-signing
    // requires the native same-team API above, so this is not a Store fallback.
    char requirement[512];
    snprintf(requirement, sizeof(requirement),
        "anchor apple generic and identifier \"%s\" and certificate leaf[subject.OU] = \"%s\"",
        identifier, signing_team);
    return xpc_connection_set_peer_code_signing_requirement(connection, requirement);
}

// Broker-side preflight for layout-only/ad-hoc bundles. Both XPC peers still
// authenticate each other independently before accessing protocol descriptors.
bool qp_xpc_has_signing_team(void) {
    return load_signing_team();
}

static bool sandbox_entitlements(void) {
    SecTaskRef task = SecTaskCreateFromSelf(NULL);
    if (!task) return false;
    CFTypeRef enabled = SecTaskCopyValueForEntitlement(task,
        CFSTR("com.apple.security.app-sandbox"), NULL);
    bool valid = enabled && CFEqual(enabled, kCFBooleanTrue);
    if (enabled) CFRelease(enabled);
    const CFStringRef forbidden[] = {
        CFSTR("com.apple.security.inherit"),
        CFSTR("com.apple.security.network.client"), CFSTR("com.apple.security.network.server"),
        CFSTR("com.apple.security.files.user-selected.read-only"),
        CFSTR("com.apple.security.files.user-selected.read-write"),
        CFSTR("com.apple.security.application-groups")
    };
    for (size_t i = 0; i < sizeof(forbidden)/sizeof(forbidden[0]); i++) {
        CFTypeRef value = SecTaskCopyValueForEntitlement(task, forbidden[i], NULL);
        if (value) { valid = false; CFRelease(value); }
    }
    CFRelease(task);
    return valid;
}

static void accept_peer(xpc_connection_t peer) {
    __block int pending_document = -1;
    __block bool owner = false;
    if (__builtin_available(macOS 12.0, *)) {
        if (set_peer_identity(peer, proxy_identifier)) {
            xpc_connection_cancel(peer); return;
        }
    } else { _exit(1); }
    xpc_connection_set_event_handler(peer, ^(xpc_object_t message) {
        if (xpc_get_type(message) == XPC_TYPE_ERROR) {
            // Separate dispatch queue: native PDF work cannot block broker-loss exit.
            if (owner) _exit(0);
            xpc_connection_cancel(peer);
            return;
        }
        if (xpc_get_type(message) != XPC_TYPE_DICTIONARY ||
            xpc_dictionary_get_uint64(message, "version") != 1) { _exit(1); }
        if (xpc_dictionary_get_bool(message, "run")) {
            if (pending_document < 0) _exit(1);
            int owned_document = pending_document;
            pending_document = -1;
            dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
                qp_renderer_run(owned_document);
                _exit(0);
            });
            return;
        }
        if (atomic_exchange(&accepted, true)) _exit(1);
        owner = true;
        int input = xpc_dictionary_dup_fd(message, "input");
        int output = xpc_dictionary_dup_fd(message, "output");
        int document = xpc_dictionary_dup_fd(message, "document");
        if (input < 0 || output < 0 || document < 0 ||
            (fcntl(document, F_GETFL) & O_ACCMODE) != O_RDONLY ||
            dup2(input, STDIN_FILENO) < 0 || dup2(output, STDOUT_FILENO) < 0) { _exit(1); }
        close(input); close(output);
        for (size_t i = 0; i < sizeof(probe_keys)/sizeof(probe_keys[0]); i++) {
            const char *value = xpc_dictionary_get_string(message, probe_keys[i]);
            if (value) {
                if (strlen(value) > 4096 || setenv(probe_keys[i], value, 1)) _exit(1);
            }
        }
        xpc_object_t reply = xpc_dictionary_create_reply(message);
        if (!reply) _exit(1);
        xpc_dictionary_set_bool(reply, "ready", true);
        xpc_connection_send_message(peer, reply);
        xpc_release(reply);
        pending_document = document;
    });
    xpc_connection_activate(peer);
}

void qp_xpc_service(void) {
    if (!sandbox_entitlements()) { os_log_error(OS_LOG_DEFAULT, "Renderer bootstrap: sandbox entitlements rejected"); _exit(1); }
    if (!load_signing_team()) { os_log_error(OS_LOG_DEFAULT, "Renderer bootstrap: proxy signature rejected"); _exit(1); }
    xpc_main(accept_peer);
}

int qp_xpc_proxy(void) {
    if (!__builtin_available(macOS 12.0, *)) return 1;
    if (!load_signing_team()) { fprintf(stderr, "XPC bootstrap failed: signed peer unavailable\n"); return 1; }
    xpc_connection_t connection = xpc_connection_create(service_name, NULL);
    if (!connection) return 1;
    int status = 1;
    if (__builtin_available(macOS 12.0, *)) {
        status = set_peer_identity(connection, service_name);
    }
    if (status) return 1;
    // Kept for this proxy's lifetime; callbacks may arrive after startup timeout.
    dispatch_semaphore_t ready = dispatch_semaphore_create(0);
    __block bool started = false;
    xpc_connection_set_event_handler(connection, ^(xpc_object_t event) {
        if (xpc_get_type(event) == XPC_TYPE_ERROR) {
            atomic_store(&disconnected, true);
            dispatch_semaphore_signal(ready);
        }
    });
    xpc_connection_activate(connection);
    xpc_object_t request = xpc_dictionary_create(NULL, NULL, 0);
    xpc_dictionary_set_uint64(request, "version", 1);
    xpc_dictionary_set_fd(request, "input", STDIN_FILENO);
    xpc_dictionary_set_fd(request, "output", STDOUT_FILENO);
    xpc_dictionary_set_fd(request, "document", 3);
    for (size_t i = 0; i < sizeof(probe_keys)/sizeof(probe_keys[0]); i++) {
        const char *value = getenv(probe_keys[i]);
        if (value && strlen(value) <= 4096) xpc_dictionary_set_string(request, probe_keys[i], value);
    }
    xpc_connection_send_message_with_reply(connection, request,
        dispatch_get_global_queue(QOS_CLASS_DEFAULT, 0), ^(xpc_object_t reply) {
            if (xpc_get_type(reply) == XPC_TYPE_ERROR) {
                fprintf(stderr, "XPC reply failed: invalid=%d interrupted=%d\n",
                    reply == XPC_ERROR_CONNECTION_INVALID, reply == XPC_ERROR_CONNECTION_INTERRUPTED);
            }
            started = xpc_get_type(reply) == XPC_TYPE_DICTIONARY &&
                xpc_dictionary_get_bool(reply, "ready");
            dispatch_semaphore_signal(ready);
        });
    xpc_release(request);
    close(3);
    if (dispatch_semaphore_wait(ready, dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_SEC)) || !started) {
        fprintf(stderr, "XPC bootstrap failed: service startup/authentication\n");
        xpc_connection_cancel(connection); return 1;
    }
#ifndef NDEBUG
    if (getenv("QUICK_PRESENTER_HELPER_TEST_REPORT_SERVICE")) {
        fprintf(stderr, "renderer-service-pid=%d\n", xpc_connection_get_pid(connection));
    }
#endif
    // Complete authenticated startup before a fast native fault can exit the
    // service; the broker's protocol pipes are already bounded independently.
    xpc_object_t run = xpc_dictionary_create(NULL, NULL, 0);
    xpc_dictionary_set_uint64(run, "version", 1);
    xpc_dictionary_set_bool(run, "run", true);
    xpc_connection_send_message(connection, run);
    xpc_release(run);
    while (!atomic_load(&disconnected)) {
        // The identity is obtained from libXPC, never a renderer-supplied field.
        pid_t pid = xpc_connection_get_pid(connection);
        struct rusage_info_v2 usage;
        if (pid > 0 && !proc_pid_rusage(pid, RUSAGE_INFO_V2, (rusage_info_t *)&usage) &&
            usage.ri_resident_size > 1024ULL * 1024 * 1024) {
            kill(pid, SIGKILL); return 1;
        }
        usleep(100000);
    }
    xpc_connection_cancel(connection);
    xpc_release(connection);
    return 0;
}
