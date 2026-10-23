#import <AppKit/AppKit.h>
#import <objc/runtime.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

// Main-thread only. One queued request bounds unsolicited Launch Services input.
// The original NSURL stays alive until Rust releases the candidate/active document.
typedef struct {
    NSURL *url;
    bool scoped;
} QPDocument;
static QPDocument *pending;
static bool rejected;
static bool modal_busy;

void qp_documents_set_modal_busy(bool value) { modal_busy = value; }

void qp_documents_release(void *handle) {
    QPDocument *document = handle;
    if (!document) return;
    if (document->scoped) [document->url stopAccessingSecurityScopedResource];
    [document->url release];
    free(document);
}

static void enqueue(NSURL *url) {
    const char *path = url.fileSystemRepresentation;
    if (modal_busy || pending || !url.isFileURL || !path || strlen(path) >= 4096) {
        rejected = true;
        return;
    }
    QPDocument *document = calloc(1, sizeof(*document));
    if (!document) { rejected = true; return; }
    document->url = [url retain];
    // A false result can also mean that an implicit sandbox extension suffices.
    document->scoped = [url startAccessingSecurityScopedResource];
    pending = document;
}

static void open_urls(id self, SEL command, NSApplication *application, NSArray<NSURL *> *urls) {
    (void)self; (void)command; (void)application;
    for (NSURL *url in urls) enqueue(url);
}

// Winit opts in to these selectors before converting a drag to path strings.
// Reading NSURL objects from the original pasteboard preserves sandbox authority.
@interface QPDocumentDropHandler : NSObject
- (BOOL)quickPresenterCanReadDrop:(NSPasteboard *)pasteboard;
- (BOOL)quickPresenterReadDrop:(NSPasteboard *)pasteboard;
@end

@implementation QPDocumentDropHandler
- (BOOL)quickPresenterCanReadDrop:(NSPasteboard *)pasteboard {
    return !modal_busy && [pasteboard canReadObjectForClasses:@[[NSURL class]]
        options:@{NSPasteboardURLReadingFileURLsOnlyKey: @YES}];
}
- (BOOL)quickPresenterReadDrop:(NSPasteboard *)pasteboard {
    NSArray<NSURL *> *urls = [pasteboard readObjectsForClasses:@[[NSURL class]]
        options:@{NSPasteboardURLReadingFileURLsOnlyKey: @YES}];
    if (!urls.count) return NO;
    for (NSURL *url in urls) enqueue(url);
    return YES;
}
@end

bool qp_documents_install(void) {
    if (![NSThread isMainThread]) return false;
    id delegate = NSApp.delegate;
    // Fail explicitly if a backend upgrade changes the integration boundary.
    if (!delegate || ![NSStringFromClass([delegate class]) isEqualToString:@"WinitApplicationDelegate"])
        return false;
    Class cls = [delegate class];
    SEL open = @selector(application:openURLs:);
    SEL can_drop = @selector(quickPresenterCanReadDrop:);
    SEL drop = @selector(quickPresenterReadDrop:);
    if (class_getInstanceMethod(cls, open) || class_getInstanceMethod(cls, can_drop) ||
        class_getInstanceMethod(cls, drop)) return false;
    Method can_method = class_getInstanceMethod([QPDocumentDropHandler class], can_drop);
    Method drop_method = class_getInstanceMethod([QPDocumentDropHandler class], drop);
    return class_addMethod(cls, open, (IMP)open_urls, "v@:@@") &&
        class_addMethod(cls, can_drop, method_getImplementation(can_method), method_getTypeEncoding(can_method)) &&
        class_addMethod(cls, drop, method_getImplementation(drop_method), method_getTypeEncoding(drop_method));
}

void *qp_documents_take(char *path, size_t capacity) {
    QPDocument *document = pending;
    pending = NULL;
    if (!document) return NULL;
    const char *value = document->url.fileSystemRepresentation;
    if (!value || strlen(value) >= capacity) {
        rejected = true;
        qp_documents_release(document);
        return NULL;
    }
    memcpy(path, value, strlen(value) + 1);
    return document;
}

bool qp_documents_take_rejected(void) {
    bool result = rejected;
    rejected = false;
    return result;
}
