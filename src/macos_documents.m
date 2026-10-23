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

bool qp_documents_install(void) {
    if (![NSThread isMainThread]) return false;
    id delegate = NSApp.delegate;
    // Fail explicitly if a backend upgrade changes the integration boundary.
    if (!delegate || ![NSStringFromClass([delegate class]) isEqualToString:@"WinitApplicationDelegate"])
        return false;
    Class cls = [delegate class];
    SEL selector = @selector(application:openURLs:);
    if (class_getInstanceMethod(cls, selector)) return false;
    return class_addMethod(cls, selector, (IMP)open_urls, "v@:@@");
}

void qp_documents_enqueue_path(const char *path) {
    @autoreleasepool {
        NSString *name = [[NSFileManager defaultManager] stringWithFileSystemRepresentation:path length:strlen(path)];
        if (name) enqueue([NSURL fileURLWithPath:name]);
        else rejected = true;
    }
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
