#import "../src/macos_documents.m"
#include <assert.h>

@interface WinitApplicationDelegate : NSObject <NSApplicationDelegate>
@end
@implementation WinitApplicationDelegate
@end

// Return a particular URL object to detect any path-to-URL reconstruction.
@interface QPOriginalURLPasteboard : NSObject {
    NSArray<NSURL *> *_urls;
}
- (instancetype)initWithURL:(NSURL *)url;
- (NSArray *)readObjectsForClasses:(NSArray *)classes options:(NSDictionary *)options;
@end
@implementation QPOriginalURLPasteboard
- (instancetype)initWithURL:(NSURL *)url {
    if ((self = [super init])) _urls = [[NSArray alloc] initWithObjects:url, nil];
    return self;
}
- (NSArray *)readObjectsForClasses:(NSArray *)classes options:(NSDictionary *)options {
    assert([classes containsObject:[NSURL class]]);
    assert([options[NSPasteboardURLReadingFileURLsOnlyKey] boolValue]);
    return _urls;
}
- (void)dealloc { [_urls release]; [super dealloc]; }
@end

int main(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        WinitApplicationDelegate *delegate = [WinitApplicationDelegate new];
        NSApp.delegate = delegate;
        assert(qp_documents_install());
        // Never replace an existing backend implementation on a later initialization.
        assert(!qp_documents_install());
        NSURL *first = [NSURL fileURLWithPath:@"/tmp/Finder first.pdf"];
        NSURL *second = [NSURL fileURLWithPath:@"/tmp/Finder second.pdf"];
        [delegate application:NSApp openURLs:@[first, second]];
        assert(qp_documents_take_rejected());
        assert(!qp_documents_take_rejected());
        char path[4096];
        void *handle = qp_documents_take(path, sizeof(path));
        assert(handle && strcmp(path, "/tmp/Finder first.pdf") == 0);
        assert(!qp_documents_take(path, sizeof(path)));
        qp_documents_release(handle);
        qp_documents_set_modal_busy(true);
        [delegate application:NSApp openURLs:@[second]];
        assert(!qp_documents_take(path, sizeof(path)));
        assert(qp_documents_take_rejected());
        qp_documents_set_modal_busy(false);
        // A later event is accepted after draining, as in a running application.
        [delegate application:NSApp openURLs:@[second]];
        handle = qp_documents_take(path, sizeof(path));
        assert(handle && strcmp(path, "/tmp/Finder second.pdf") == 0);
        qp_documents_release(handle);
        [delegate application:NSApp openURLs:@[[NSURL URLWithString:@"https://example.org/file.pdf"]]];
        assert(!qp_documents_take(path, sizeof(path)));
        assert(qp_documents_take_rejected());
        [delegate application:NSApp openURLs:@[[NSURL fileURLWithPath:@"/tmp/drop.pdf"]]];
        assert(!qp_documents_take(path, 2));
        assert(qp_documents_take_rejected());
        [delegate application:NSApp openURLs:@[[NSURL fileURLWithPath:@"/tmp/drop.pdf"]]];
        handle = qp_documents_take(path, sizeof(path));
        assert(handle && strcmp(path, "/tmp/drop.pdf") == 0);
        qp_documents_release(handle);
        // An open event can outlive the file. Forward its exact path so the
        // renderer can fail the candidate while retaining the active document.
        NSString *missing = [NSTemporaryDirectory() stringByAppendingPathComponent:
            [NSString stringWithFormat:@"qp492-missing-%@.pdf", NSUUID.UUID.UUIDString]];
        assert(![[NSFileManager defaultManager] fileExistsAtPath:missing]);
        [delegate application:NSApp openURLs:@[[NSURL fileURLWithPath:missing]]];
        handle = qp_documents_take(path, sizeof(path));
        assert(handle && strcmp(path, missing.fileSystemRepresentation) == 0);
        assert(!qp_documents_take_rejected());
        qp_documents_release(handle);
        QPDocumentDropHandler *drop_delegate = (QPDocumentDropHandler *)delegate;
        NSPasteboard *pasteboard = [NSPasteboard pasteboardWithUniqueName];
        assert(![drop_delegate quickPresenterCanReadDrop:pasteboard]);
        assert(![drop_delegate quickPresenterReadDrop:pasteboard]);
        [pasteboard writeObjects:@[[NSURL fileURLWithPath:@"/tmp/drop-original.pdf"]]];
        assert([drop_delegate quickPresenterCanReadDrop:pasteboard]);
        qp_documents_set_modal_busy(true);
        assert(![drop_delegate quickPresenterCanReadDrop:pasteboard]);
        assert([drop_delegate quickPresenterReadDrop:pasteboard]);
        assert(qp_documents_take_rejected());
        assert(!qp_documents_take(path, sizeof(path)));
        qp_documents_set_modal_busy(false);
        assert([drop_delegate quickPresenterReadDrop:pasteboard]);
        [pasteboard clearContents];
        handle = qp_documents_take(path, sizeof(path));
        assert(handle && strcmp(path, "/tmp/drop-original.pdf") == 0);
        qp_documents_release(handle);
        [pasteboard releaseGlobally];
        NSURL *original = [[NSURL alloc] initFileURLWithPath:@"/tmp/drop-granted-original.pdf"];
        QPOriginalURLPasteboard *original_pasteboard = [[QPOriginalURLPasteboard alloc] initWithURL:original];
        assert([drop_delegate quickPresenterReadDrop:(NSPasteboard *)original_pasteboard]);
        [original_pasteboard release];
        handle = qp_documents_take(path, sizeof(path));
        assert(handle && ((QPDocument *)handle)->url == original);
        [original release];
        assert(strcmp(((QPDocument *)handle)->url.fileSystemRepresentation,
            "/tmp/drop-granted-original.pdf") == 0);
        qp_documents_release(handle);

        NSApp.delegate = nil;
        [delegate release];
    }
    return 0;
}
