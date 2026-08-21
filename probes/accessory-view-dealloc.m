/* Does -[NSSavePanel setAccessoryView:nil] actually let the accessory view
 *
 * MEASURED ANSWER (macOS 26.6.1, unlocked, n=3, 20 presentations per run):
 *   release popup with accessoryView still set  -> 0/20 deallocations, 932.2 KB/cycle
 *   setAccessoryView:nil first, THEN release    -> 19/20 deallocations, 542.3 KB/cycle
 * So giving back the caller's +1 without clearing the panel's reference frees
 * NOTHING -- the footprint is indistinguishable from doing nothing at all -- while
 * the correct ordering reclaims ~390 KB per invocation. The failure mode is the
 * ORDERING, not the release, and the wrong version is exactly what someone writes
 * reasoning only from "we own a +1, so give it back". It looks right, it passes
 * review, and it reclaims nothing.
 *
 * The number that makes it a mechanism rather than a figure: the cleared-mode
 * residual (542.3) lands on the independently measured bare-panel cost from
 * appkit-panel-control.m (518.5, no accessory view at all). With the popup
 * genuinely freed, what remains is the pinned panel and nothing else. The two
 * probes share no code path.
 *
 * Not rounded: it is 19/20, not 20/20. One popup per run does not deallocate. Not
 * investigated; the untested guess is first-presentation warm-up or an autorelease
 * pool undrained at report time. Nineteen against zero carries the verdict without
 * needing to be twenty, and the entry should not say "all".
 *
 * Original question:
 * Does -[NSSavePanel setAccessoryView:nil] actually let the accessory view
 * deallocate, or does releasing it while the panel still references it merely
 * balance the books without freeing anything?
 *
 * Emulates GTK's ownership exactly: the popup is alloc'd (+1 the caller owns) and
 * handed to setAccessoryView: (the panel takes its own retain). The panel itself
 * is deliberately NOT released -- GTK does not, and it is pinned by _retainedSelf
 * regardless.
 *
 *   --keep  : [pop release] with the panel still holding accessoryView
 *   --clear : [panel setAccessoryView:nil] first, THEN [pop release]
 *
 * A dealloc spy (associated object on the POPUP) counts actual frees.
 */
#import <AppKit/AppKit.h>
#import <objc/runtime.h>
#include <mach/mach.h>
#include <stdio.h>
#include <string.h>

@interface Spy : NSObject @end
static int pop_deallocs = 0;
@implementation Spy
- (void)dealloc { pop_deallocs++; [super dealloc]; }
@end
static char key;

static int clear_first = 0, cycles = 0, total = 20;
static NSSavePanel *panel = nil;
static NSPopUpButton *pop = nil;
static long rss_base = 0;

static long rss_kb(void) {
    mach_task_basic_info_data_t bi; mach_msg_type_number_t c = MACH_TASK_BASIC_INFO_COUNT;
    task_info(mach_task_self(), MACH_TASK_BASIC_INFO, (task_info_t)&bi, &c);
    return bi.resident_size / 1024;
}
static void run_cycle(void);
static void after_cancel(void) {
    [panel orderOut:nil];
    if (clear_first)
        [panel setAccessoryView:nil];
    [pop release];              /* the release GTK never issues */
    pop = nil;
    panel = nil;                /* panel deliberately leaked, as GTK leaks it */
    cycles++;
    if (cycles == 1) rss_base = rss_kb();
    if (cycles == total) {
        long grown = rss_kb() - rss_base;
        printf("mode=%-6s cycles=%d popup_deallocs=%d/%d  rss %+ld KB  per_cycle=%.1f KB\n",
               clear_first ? "clear" : "keep", total, pop_deallocs, total,
               grown, (double)grown / (total - 1));
        fflush(stdout);
        [NSApp terminate:nil];
        return;
    }
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 150 * NSEC_PER_MSEC),
                   dispatch_get_main_queue(), ^{ run_cycle(); });
}
static void run_cycle(void) {
    panel = [[NSSavePanel savePanel] retain];
    [panel setReleasedWhenClosed:NO];
    [panel setNameFieldStringValue:@"probe.pdf"];
    [panel setDirectoryURL:[NSURL fileURLWithPath:@"/private/tmp"]];
    pop = [[NSPopUpButton alloc] initWithFrame:NSMakeRect(0, 0, 200, 24) pullsDown:NO];
    [pop addItemsWithTitles:@[ @"PDF files", @"All files" ]];
    Spy *s = [[Spy alloc] init];
    objc_setAssociatedObject(pop, &key, s, OBJC_ASSOCIATION_RETAIN);
    [s release];
    [panel setAccessoryView:pop];
    [panel beginWithCompletionHandler:^(NSModalResponse r) { (void)r; after_cancel(); }];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 600 * NSEC_PER_MSEC),
                   dispatch_get_main_queue(), ^{ [panel cancel:nil]; });
}
int main(int argc, char **argv) {
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--clear")) clear_first = 1;
        else if (!strcmp(argv[i], "--cycles") && i + 1 < argc) total = atoi(argv[++i]);
    }
    @autoreleasepool {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        dispatch_async(dispatch_get_main_queue(), ^{ run_cycle(); });
        [NSApp run];
    }
    return 0;
}
