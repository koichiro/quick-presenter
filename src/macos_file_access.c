#include <CoreFoundation/CoreFoundation.h>
#include <Security/Security.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

// Only the UI calls this bridge. Bookmarks are never passed to PDFium or XPC.
bool qp_ui_is_sandboxed(void) {
    SecCodeRef code = NULL;
    CFDictionaryRef info = NULL;
    bool enabled = false;
    if (SecCodeCopySelf(kSecCSDefaultFlags, &code) == errSecSuccess &&
        SecCodeCopySigningInformation(code, kSecCSSigningInformation, &info) == errSecSuccess) {
        CFDictionaryRef entitlements = CFDictionaryGetValue(info, kSecCodeInfoEntitlementsDict);
        if (entitlements) {
            CFTypeRef value = CFDictionaryGetValue(entitlements, CFSTR("com.apple.security.app-sandbox"));
            enabled = value && CFEqual(value, kCFBooleanTrue);
        }
    }
    if (info) CFRelease(info);
    if (code) CFRelease(code);
    return enabled;
}

void *qp_ui_bookmark_create(const char *path, size_t *length) {
    CFURLRef url = CFURLCreateFromFileSystemRepresentation(NULL, (const UInt8 *)path, strlen(path), false);
    if (!url) return NULL;
    CFDataRef data = CFURLCreateBookmarkData(NULL, url,
        kCFURLBookmarkCreationWithSecurityScope | kCFURLBookmarkCreationSecurityScopeAllowOnlyReadAccess,
        NULL, NULL, NULL);
    CFRelease(url);
    if (!data) return NULL;
    *length = (size_t)CFDataGetLength(data);
    void *bytes = malloc(*length);
    if (bytes) memcpy(bytes, CFDataGetBytePtr(data), *length);
    CFRelease(data);
    return bytes;
}

void *qp_ui_bookmark_open(const unsigned char *bytes, size_t length, unsigned char *path, size_t capacity, bool *stale) {
    CFDataRef data = CFDataCreate(NULL, bytes, (CFIndex)length);
    if (!data) return NULL;
    Boolean is_stale = false;
    CFURLRef url = CFURLCreateByResolvingBookmarkData(NULL, data,
        kCFURLBookmarkResolutionWithSecurityScope | kCFURLBookmarkResolutionWithoutUIMask,
        NULL, NULL, &is_stale, NULL);
    CFRelease(data);
    if (!url) return NULL;
    if (!CFURLGetFileSystemRepresentation(url, true, path, (CFIndex)capacity) ||
        !CFURLStartAccessingSecurityScopedResource(url)) {
        CFRelease(url);
        return NULL;
    }
    *stale = is_stale;
    return (void *)url;
}

void qp_ui_bookmark_close(void *handle) {
    CFURLStopAccessingSecurityScopedResource((CFURLRef)handle);
    CFRelease(handle);
}

void qp_ui_bookmark_free(void *bytes) { free(bytes); }
