/* Pure AppKit control for the GTK Quartz file-chooser footprint investigation.
 *
 * This file exists because the FIRST version of this control was wrong in a way
 * that flattered a conclusion. It dismissed the panel with -close while GTK's
 * completion path sends only -orderOut:, so control and treatment differed in two
 * ways at once (GTK-vs-no-GTK, and close-vs-orderOut) and the entire difference
 * was charged to GTK. Corrected, the control inverts the conclusion: a process
 * with no GTK in it at all grows ~0.56 MB per panel presentation, ~0.97 MB with
 * an accessory view, linearly to at least 40 cycles, and the panel is never
 * deallocated whichever dismissal is used. The cost is AppKit's.
 *
 * Every switch below is a variable someone asked to hold still: --dismiss
 * isolates close from orderOut:, --accessory isolates the filter popup GTK
 * installs, and --no-spy exists to prove the dealloc instrument is not itself
 * causing what it measures.
 *
 * Build:
 *   clang -ObjC -O1 -o /tmp/appkit-panel-control probes/appkit-panel-control.m \
 *       -framework AppKit
 *
 * --invalidate and the _retainedSelf dump read PRIVATE AppKit state. That is
 * legitimate in a probe and nowhere else: it is how the second retainer was
 * named. On macOS 26.6.1 NSSavePanel holds a strong reference to ITSELF in an
 * ivar called _retainedSelf, still set after the completion handler has run and
 * after the panel is ordered out or closed -- which is why nothing anyone
 * releases, in GTK or in the application, ever deallocates it.
 * No GTK, no GLib: an NSApplication presenting and dismissing panels.
 *
 * --dismiss close    : -close   (what the first version of this control did)
 * --dismiss orderout : -orderOut: ONLY, which is what GTK's completion handler
 *                      actually sends (gtkfilechoosernativequartz.c:374)
 *
 * The distinction is the point. Comparing GTK-with-orderOut against
 * AppKit-with-close varies two things at once and charges the whole difference
 * to "GTK", which is the error this file exists to correct.
 */
#import <AppKit/AppKit.h>
#import <objc/runtime.h>
#include <mach/mach.h>
#include <stdio.h>
#include <string.h>

@interface DeallocSpy : NSObject
@end
static int panel_deallocs = 0;
@implementation DeallocSpy
- (void)dealloc { panel_deallocs++; [super dealloc]; }
@end
static char spy_key;

static int cycles = 0, total = 20;
static int use_close = 1;
static int use_accessory = 0;
static int use_spy = 1;
static int use_invalidate = 0;
static NSSavePanel *panel = nil;
static NSSavePanel *last_panel = nil;
static long rss_base = 0;

static long rss_kb(void) {
    mach_task_basic_info_data_t bi; mach_msg_type_number_t c = MACH_TASK_BASIC_INFO_COUNT;
    task_info(mach_task_self(), MACH_TASK_BASIC_INFO, (task_info_t)&bi, &c);
    return bi.resident_size / 1024;
}
static int panel_count(void) {
    int n = 0;
    for (NSWindow *w in [NSApp windows]) if ([w isKindOfClass:[NSSavePanel class]]) n++;
    return n;
}
static void run_cycle(void);
static void after_cancel(void) {
    if (use_close)
        [panel close];
    else
        [panel orderOut:nil];   /* GTK's actual dismissal */
    if (use_invalidate) {
        /* PROBE ONLY, never shippable: drive the remote-view teardown the
         * out-of-process powerbox uses, to test whether the XPC bridge is what
         * pins the panel. */
        SEL sel = NSSelectorFromString(@"_invalidateRemoteView:");
        if ([panel respondsToSelector:sel]) {
            IMP imp = [panel methodForSelector:sel];
            ((void (*)(id, SEL, id))imp)(panel, sel, nil);
        } else if (cycles == 0) {
            printf("  note: -_invalidateRemoteView: not present on %s\n",
                   class_getName([panel class]));
        }
    }
    last_panel = panel;
    panel = nil;
    cycles++;
    if (cycles <= 2) {
        /* macOS 26.6.1 NSSavePanel carries an ivar literally named _retainedSelf,
         * plus _panelCompleted / _panelIsNowUseless / _didBeginServicePanel, and
         * conforms to NSOSPHostExportedToServiceProtocol. If the panel retains
         * ITSELF for the service session, that is the second retainer -- in
         * AppKit, in-process, and beyond anything GTK or we can balance. */
        Ivar iv = class_getInstanceVariable([last_panel class], "_retainedSelf");
        id rs = iv ? object_getIvar(last_panel, iv) : nil;
        printf("  [cycle %d] _retainedSelf=%p (panel=%p, same=%s)", cycles,
               (void *)rs, (void *)last_panel, rs == last_panel ? "YES" : "no");
        const char *bools[] = {"_panelCompleted", "_panelIsNowUseless", "_didBeginServicePanel",
                               "_observingBridge", NULL};
        for (int i = 0; bools[i]; i++) {
            Ivar b = class_getInstanceVariable([last_panel class], bools[i]);
            if (b) printf(" %s=%d", bools[i],
                          *(BOOL *)((char *)last_panel + ivar_getOffset(b)));
        }
        printf("\n"); fflush(stdout);
    }
    if (cycles == 1) rss_base = rss_kb();   /* baseline after warm-up cycle */
    if (0)
        printf("  [%s] cycle %2d: rss %ld KB (%+ld since baseline), live_panels %d, deallocs %d\n",
               use_close ? "close" : "orderout", cycles, rss_kb(), rss_kb() - rss_base,
               panel_count(), panel_deallocs), fflush(stdout);
    if (cycles == total) {
        long grown = rss_kb() - rss_base;
        printf("spy=%d accessory=%d dismiss=%s cycles=%d rss_total=%+ld KB per_cycle=%.1f KB live_panels=%d deallocs=%d\n",
               use_spy, use_accessory, use_close ? "close" : "orderout", total, grown,
               (double)grown / (total - 1), panel_count(), panel_deallocs);
        fflush(stdout);
        [NSApp terminate:nil];
        return;
    }
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 200 * NSEC_PER_MSEC),
                   dispatch_get_main_queue(), ^{ run_cycle(); });
}
static void run_cycle(void) {
    panel = [[NSSavePanel savePanel] retain];   /* GTK's retain, same shape */
    [panel setReleasedWhenClosed:YES];
    [panel setNameFieldStringValue:@"probe.pdf"];
    [panel setDirectoryURL:[NSURL fileURLWithPath:@"/private/tmp"]];
    if (use_spy) {
        DeallocSpy *spy = [[DeallocSpy alloc] init];
        objc_setAssociatedObject(panel, &spy_key, spy, OBJC_ASSOCIATION_RETAIN);
        [spy release];
    }
    if (use_accessory) {
        /* Same shape GTK installs: an NSPopUpButton of filter names, alloc'd and
         * handed to the panel. gtkfilechoosernativequartz.c:314/350. */
        NSPopUpButton *pop = [[NSPopUpButton alloc] initWithFrame:NSMakeRect(0, 0, 200, 26)
                                                       pullsDown:NO];
        [pop addItemsWithTitles:@[@"PDF files", @"All files"]];
        [panel setAccessoryView:pop];
        /* deliberately NOT released -- mirrors GTK's unbalanced +1 */
    }
    [panel beginWithCompletionHandler:^(NSModalResponse r) { (void)r; after_cancel(); }];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 700 * NSEC_PER_MSEC),
                   dispatch_get_main_queue(), ^{ [panel cancel:nil]; });
}
int main(int argc, char **argv) {
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--dismiss") && i + 1 < argc) use_close = !strcmp(argv[++i], "close");
        else if (!strcmp(argv[i], "--cycles") && i + 1 < argc) total = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--accessory")) use_accessory = 1;
        else if (!strcmp(argv[i], "--no-spy")) use_spy = 0;
        else if (!strcmp(argv[i], "--invalidate")) use_invalidate = 1;
    }
    @autoreleasepool {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        dispatch_async(dispatch_get_main_queue(), ^{ run_cycle(); });
        [NSApp run];
    }
    return 0;
}
