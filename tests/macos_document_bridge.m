#import "../src/macos_documents.m"
#include <assert.h>

@interface WinitApplicationDelegate : NSObject <NSApplicationDelegate>
@end
@implementation WinitApplicationDelegate
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
        qp_documents_enqueue_path("/tmp/drop.pdf");
        assert(!qp_documents_take(path, 2));
        assert(qp_documents_take_rejected());
        qp_documents_enqueue_path("/tmp/drop.pdf");
        handle = qp_documents_take(path, sizeof(path));
        assert(handle && strcmp(path, "/tmp/drop.pdf") == 0);
        qp_documents_release(handle);
        NSApp.delegate = nil;
        [delegate release];
    }
    return 0;
}
