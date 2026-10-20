#import <Foundation/Foundation.h>
#include <Security/Security.h>
#include <bsm/libbsm.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>
#include <ctype.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

// Experiment only: socket tokens are not per-message XPC authentication.
struct probe_context {
    char path[1024];
    char team[64];
    char group[128];
};
struct probe_peer { uint32_t token[8]; };

static bool copy_string(CFTypeRef value, char *dest, size_t capacity) {
    return value && CFGetTypeID(value) == CFStringGetTypeID() &&
        CFStringGetCString(value, dest, capacity, kCFStringEncodingUTF8);
}

static int validate(SecCodeRef code, const char *identifier,
                    const struct probe_context *context, OSStatus *status) {
    NSString *rule = @"anchor apple generic";
    if (identifier) rule = [rule stringByAppendingFormat:@" and identifier \"%s\"", identifier];
    SecRequirementRef requirement = NULL;
    *status = SecRequirementCreateWithString((__bridge CFStringRef)rule,
        kSecCSDefaultFlags, &requirement);
    if (*status != errSecSuccess) return 2;
    *status = SecCodeCheckValidity(code, kSecCSDefaultFlags, requirement);
    CFRelease(requirement);
    if (*status != errSecSuccess) return 3;
    CFDictionaryRef info = NULL;
    *status = SecCodeCopySigningInformation(code, kSecCSSigningInformation, &info);
    if (*status != errSecSuccess) return 4;
    char team[64] = {0};
    bool valid = copy_string(CFDictionaryGetValue(info, kSecCodeInfoTeamIdentifier), team, sizeof(team)) &&
        strcmp(team, context->team) == 0;
    CFTypeRef entitlements = CFDictionaryGetValue(info, kSecCodeInfoEntitlementsDict);
    if (!entitlements || CFGetTypeID(entitlements) != CFDictionaryGetTypeID()) valid = false;
    else {
        CFDictionaryRef ent = (CFDictionaryRef)entitlements;
        CFTypeRef sandbox = CFDictionaryGetValue(ent, CFSTR("com.apple.security.app-sandbox"));
        CFTypeRef groups = CFDictionaryGetValue(ent, CFSTR("com.apple.security.application-groups"));
        CFStringRef expected = CFStringCreateWithCString(NULL, context->group, kCFStringEncodingUTF8);
        valid = valid && sandbox && CFEqual(sandbox, kCFBooleanTrue) && groups && expected &&
            CFGetTypeID(groups) == CFArrayGetTypeID() &&
            CFArrayGetCount((CFArrayRef)groups) == 1 &&
            CFEqual(CFArrayGetValueAtIndex((CFArrayRef)groups, 0), expected);
        if (expected) CFRelease(expected);
        const CFStringRef forbidden[] = {
            CFSTR("com.apple.security.inherit"), CFSTR("com.apple.security.network.client"),
            CFSTR("com.apple.security.network.server"), CFSTR("com.apple.security.files.bookmarks.app-scope"),
            CFSTR("com.apple.security.files.user-selected.read-only"),
            CFSTR("com.apple.security.files.user-selected.read-write")
        };
        for (size_t i = 0; i < sizeof(forbidden)/sizeof(forbidden[0]); i++)
            if (CFDictionaryContainsKey(ent, forbidden[i])) valid = false;
    }
    CFRelease(info);
    return valid ? 0 : 5;
}

int qp_probe_context(struct probe_context *context, OSStatus *status) {
    @autoreleasepool {
        if (@available(macOS 14.4, *)) {} else return 1;
        SecCodeRef code = NULL;
        CFDictionaryRef info = NULL;
        *status = SecCodeCopySelf(kSecCSDefaultFlags, &code);
        if (*status != errSecSuccess) return 2;
        *status = SecCodeCopySigningInformation(code, kSecCSSigningInformation, &info);
        bool valid = *status == errSecSuccess && copy_string(
            CFDictionaryGetValue(info, kSecCodeInfoTeamIdentifier), context->team, sizeof(context->team));
        if (info) CFRelease(info);
        for (size_t i = 0; valid && context->team[i]; i++)
            if (!isalnum((unsigned char)context->team[i])) valid = false;
        if (!valid || !context->team[0]) { CFRelease(code); return 4; }
        snprintf(context->group, sizeof(context->group), "%s.app.quickpresenter.control-probe", context->team);
        // A trusted wrong-identifier copy is deliberately allowed here for negative peer tests.
        // Remote identity is still pinned independently by each endpoint.
        int result = validate(code, NULL, context, status);
        CFRelease(code);
        if (result) return result;
        NSURL *url = [[NSFileManager defaultManager]
            containerURLForSecurityApplicationGroupIdentifier:[NSString stringWithUTF8String:context->group]];
        if (!url || ![url getFileSystemRepresentation:context->path maxLength:sizeof(context->path)]) return 6;
        return 0;
    }
}

int qp_probe_peer(int fd, const char *identifier, const struct probe_context *context,
                  struct probe_peer *peer, OSStatus *status) {
    @autoreleasepool {
        uid_t uid; gid_t gid;
        if (getpeereid(fd, &uid, &gid) || uid != geteuid()) return 7;
        audit_token_t token;
        socklen_t length = sizeof(token);
        if (getsockopt(fd, SOL_LOCAL, LOCAL_PEERTOKEN, &token, &length) ||
            length != sizeof(token) || audit_token_to_euid(token) != geteuid()) return 8;
        CFDataRef data = CFDataCreate(NULL, (const UInt8 *)&token, sizeof(token));
        const void *key = kSecGuestAttributeAudit;
        const void *value = data;
        CFDictionaryRef attributes = CFDictionaryCreate(NULL, &key, &value, 1,
            &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
        SecCodeRef code = NULL;
        *status = SecCodeCopyGuestWithAttributes(NULL, attributes, kSecCSDefaultFlags, &code);
        CFRelease(attributes); CFRelease(data);
        if (*status != errSecSuccess) return 9;
        int result = validate(code, identifier, context, status);
        CFRelease(code);
        if (!result) memcpy(peer->token, &token, sizeof(token));
        return result;
    }
}
