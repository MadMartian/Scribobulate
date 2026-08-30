/*
 * native-chooser-rss.c -- attribute the macOS per-invocation footprint growth of
 * GtkFileChooserNative: RSS that climbs with every open/close of the native chooser
 * and never comes back.
 *
 * (Deliberately no ISSUES.md citation. An issue exists in order to be deleted, so a
 * pointer from outside that register dangles the moment the fix lands -- and lies
 * quietly if the letters are ever compacted. SDD principle 6 / POLICY.md's cross-
 * reference gate. The subject is named above instead, which needs no register at all.)
 *
 * The measurement that raised the issue read RSS from outside the process, which
 * says THAT it grows and nothing about WHERE. This probe answers "where" without
 * needing task_for_pid: it walks its own VM map with mach_vm_region_recurse and
 * sums dirty pages per user tag (the same accounting `vmmap -summary` prints),
 * and it reads every malloc zone's in-use bytes. A checkpoint diff then says
 * whether the growth is malloc'd heap at all -- which is the first fork in the
 * road, and the one that decides whether MallocStackLogging is even the right
 * instrument.
 *
 * Cycle: construct a Save chooser shaped like window/export.rs's
 * choose_destination (filter + current name + current folder + transient parent),
 * show it, dismiss it, destroy it, drop the last ref.
 *
 * It is ObjC rather than C for one reason: --cancel drives the SAME dismissal the
 * user's Cancel button drives, by sending -[NSSavePanel cancel:] to the live
 * panel. That matters because gtk_native_dialog_hide() takes a DIFFERENT path in
 * the Quartz backend (skip_response), so a probe that dismissed with hide() would
 * be measuring a path the application never takes. It also needs no input events,
 * so it is valid with the screen locked, where a keystroke-driven probe is not.
 *
 * Build:
 *   clang -ObjC -O1 -g -o /tmp/native-chooser-rss probes/native-chooser-rss.m \
 *       $(pkg-config --cflags --libs gtk4) -framework AppKit
 * Run:
 *   /tmp/native-chooser-rss [--cycles N] [--hold MS] [--gap MS] [--every N]
 */
#include <gtk/gtk.h>
#import <AppKit/AppKit.h>
#import <objc/runtime.h>
/* objc_autoreleasePoolPush/Pop are exported by libobjc but declared only in a
 * private header, so declare them here rather than reaching for that header. */
extern void *objc_autoreleasePoolPush(void);
extern void objc_autoreleasePoolPop(void *);
#include <mach/mach.h>
#include <mach/mach_vm.h>
#include <malloc/malloc.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>

#define MAX_TAG 256

typedef struct {
    unsigned long long dirty_kb[MAX_TAG];
    unsigned long long resident_kb[MAX_TAG];
    unsigned long long malloc_in_use;
    unsigned long long malloc_size_allocated;
    unsigned long long footprint_kb;
    unsigned long long rss_kb;
} snapshot;

static const char *tag_name(unsigned t)
{
    switch (t) {
    case 0: return "(none/anon)";
    case VM_MEMORY_MALLOC: return "MALLOC";
    case VM_MEMORY_MALLOC_SMALL: return "MALLOC_SMALL";
    case VM_MEMORY_MALLOC_LARGE: return "MALLOC_LARGE";
    case VM_MEMORY_MALLOC_HUGE: return "MALLOC_HUGE";
    case VM_MEMORY_SBRK: return "SBRK";
    case VM_MEMORY_REALLOC: return "REALLOC";
    case VM_MEMORY_MALLOC_TINY: return "MALLOC_TINY";
    case VM_MEMORY_MALLOC_LARGE_REUSABLE: return "MALLOC_LARGE_REUSABLE";
    case VM_MEMORY_MALLOC_LARGE_REUSED: return "MALLOC_LARGE_REUSED";
    case VM_MEMORY_ANALYSIS_TOOL: return "ANALYSIS_TOOL";
    case VM_MEMORY_MALLOC_NANO: return "MALLOC_NANO";
    case VM_MEMORY_MALLOC_MEDIUM: return "MALLOC_MEDIUM";
    case VM_MEMORY_STACK: return "STACK";
    case VM_MEMORY_GUARD: return "GUARD";
    case VM_MEMORY_SHARED_PMAP: return "SHARED_PMAP";
    case VM_MEMORY_DYLIB: return "DYLIB";
    case VM_MEMORY_OBJC_DISPATCHERS: return "OBJC_DISPATCHERS";
    case VM_MEMORY_UNSHARED_PMAP: return "UNSHARED_PMAP";
    case VM_MEMORY_APPKIT: return "APPKIT";
    case VM_MEMORY_FOUNDATION: return "FOUNDATION";
    case VM_MEMORY_COREGRAPHICS: return "COREGRAPHICS";
    case VM_MEMORY_CORESERVICES: return "CORESERVICES/CARBON";
    case VM_MEMORY_JAVA: return "JAVA";
    case VM_MEMORY_COREDATA: return "COREDATA";
    case VM_MEMORY_ATS: return "ATS(font)";
    case VM_MEMORY_LAYERKIT: return "LAYERKIT/CoreAnimation";
    case VM_MEMORY_CGIMAGE: return "CGIMAGE";
    case VM_MEMORY_TCMALLOC: return "TCMALLOC";
    case VM_MEMORY_COREGRAPHICS_DATA: return "CG_DATA";
    case VM_MEMORY_COREGRAPHICS_SHARED: return "CG_SHARED";
    case VM_MEMORY_COREGRAPHICS_FRAMEBUFFERS: return "CG_FRAMEBUFFERS";
    case VM_MEMORY_COREGRAPHICS_BACKINGSTORES: return "CG_BACKINGSTORES";
    case VM_MEMORY_DYLD: return "DYLD";
    case VM_MEMORY_DYLD_MALLOC: return "DYLD_MALLOC";
    case VM_MEMORY_SQLITE: return "SQLITE";
    case VM_MEMORY_JAVASCRIPT_CORE: return "JS_CORE";
    case VM_MEMORY_GLSL: return "GLSL";
    case VM_MEMORY_OPENCL: return "OPENCL";
    case VM_MEMORY_COREIMAGE: return "COREIMAGE";
    case VM_MEMORY_IMAGEIO: return "IMAGEIO";
    case VM_MEMORY_ACCELERATE: return "ACCELERATE";
    case VM_MEMORY_COREUI: return "COREUI";
    case VM_MEMORY_COREUIFILE: return "COREUIFILE";
    case VM_MEMORY_GENEALOGY: return "GENEALOGY";
    case VM_MEMORY_RAWCAMERA: return "RAWCAMERA";
    case VM_MEMORY_CORPSEINFO: return "CORPSEINFO";
    case VM_MEMORY_ASL: return "ASL";
    case VM_MEMORY_SWIFT_RUNTIME: return "SWIFT_RUNTIME";
    case VM_MEMORY_SWIFT_METADATA: return "SWIFT_METADATA";
    case VM_MEMORY_DHMM: return "DHMM";
    case VM_MEMORY_SCENEKIT: return "SCENEKIT";
    case VM_MEMORY_SKYWALK: return "SKYWALK";
    case VM_MEMORY_IOSURFACE: return "IOSURFACE";
    case VM_MEMORY_LIBNETWORK: return "LIBNETWORK";
    case VM_MEMORY_AUDIO: return "AUDIO";
    case VM_MEMORY_VIDEOBITSTREAM: return "VIDEOBITSTREAM";
    case VM_MEMORY_CM_XPC: return "CM_XPC";
    case VM_MEMORY_CM_RPC: return "CM_RPC";
    case VM_MEMORY_CM_MEMORYPOOL: return "CM_MEMORYPOOL";
    case VM_MEMORY_CM_READCACHE: return "CM_READCACHE";
    case VM_MEMORY_CM_CRABS: return "CM_CRABS";
    case VM_MEMORY_QUICKLOOK_THUMBNAILS: return "QUICKLOOK_THUMBS";
    case VM_MEMORY_ACCOUNTS: return "ACCOUNTS";
    case VM_MEMORY_SANITIZER: return "SANITIZER";
    case VM_MEMORY_IOACCELERATOR: return "IOACCELERATOR";
    default: return NULL;
    }
}

static void take_snapshot(snapshot *s)
{
    memset(s, 0, sizeof(*s));

    mach_vm_address_t address = 0;
    natural_t depth = 0;
    for (;;) {
        mach_vm_size_t size = 0;
        vm_region_submap_info_data_64_t info;
        mach_msg_type_number_t count = VM_REGION_SUBMAP_INFO_COUNT_64;
        kern_return_t kr = mach_vm_region_recurse(mach_task_self(), &address, &size,
                                                 &depth, (vm_region_recurse_info_t)&info,
                                                 &count);
        if (kr != KERN_SUCCESS)
            break;
        if (info.is_submap) {
            depth++;
            continue;
        }
        unsigned tag = info.user_tag < MAX_TAG ? info.user_tag : MAX_TAG - 1;
        s->dirty_kb[tag] += (unsigned long long)info.pages_dirtied * 4;
        s->resident_kb[tag] += (unsigned long long)info.pages_resident * 4;
        address += size;
    }

    vm_address_t *zones = NULL;
    unsigned zone_count = 0;
    if (malloc_get_all_zones(mach_task_self(), NULL, &zones, &zone_count) == KERN_SUCCESS) {
        for (unsigned i = 0; i < zone_count; i++) {
            malloc_statistics_t st;
            memset(&st, 0, sizeof(st));
            malloc_zone_statistics((malloc_zone_t *)zones[i], &st);
            s->malloc_in_use += st.size_in_use;
            s->malloc_size_allocated += st.size_allocated;
        }
    }

    mach_task_basic_info_data_t bi;
    mach_msg_type_number_t bic = MACH_TASK_BASIC_INFO_COUNT;
    if (task_info(mach_task_self(), MACH_TASK_BASIC_INFO, (task_info_t)&bi, &bic) == KERN_SUCCESS)
        s->rss_kb = bi.resident_size / 1024;

    task_vm_info_data_t vi;
    mach_msg_type_number_t vic = TASK_VM_INFO_COUNT;
    if (task_info(mach_task_self(), TASK_VM_INFO, (task_info_t)&vi, &vic) == KERN_SUCCESS)
        s->footprint_kb = vi.phys_footprint / 1024;
}

static void report_diff(const char *label, const snapshot *a, const snapshot *b, int cycles)
{
    printf("\n=== %s (%d cycles) ===\n", label, cycles);
    printf("  rss        %8lld -> %8lld KB  (%+lld KB, %+.1f KB/cycle)\n",
           a->rss_kb, b->rss_kb, (long long)(b->rss_kb - a->rss_kb),
           cycles ? (double)((long long)b->rss_kb - (long long)a->rss_kb) / cycles : 0.0);
    printf("  footprint  %8lld -> %8lld KB  (%+lld KB, %+.1f KB/cycle)\n",
           a->footprint_kb, b->footprint_kb, (long long)(b->footprint_kb - a->footprint_kb),
           cycles ? (double)((long long)b->footprint_kb - (long long)a->footprint_kb) / cycles : 0.0);
    printf("  malloc in-use %5lld -> %8lld KB  (%+lld KB, %+.1f KB/cycle)\n",
           a->malloc_in_use / 1024, b->malloc_in_use / 1024,
           (long long)(b->malloc_in_use - a->malloc_in_use) / 1024,
           cycles ? (double)((long long)b->malloc_in_use - (long long)a->malloc_in_use) / 1024.0 / cycles : 0.0);
    printf("  malloc alloc  %5lld -> %8lld KB  (%+lld KB)\n",
           a->malloc_size_allocated / 1024, b->malloc_size_allocated / 1024,
           (long long)(b->malloc_size_allocated - a->malloc_size_allocated) / 1024);
    printf("  --- dirty pages by VM tag (only tags that moved) ---\n");
    for (unsigned t = 0; t < MAX_TAG; t++) {
        long long d = (long long)b->dirty_kb[t] - (long long)a->dirty_kb[t];
        long long r = (long long)b->resident_kb[t] - (long long)a->resident_kb[t];
        if (d == 0 && r == 0)
            continue;
        const char *n = tag_name(t);
        char fallback[32];
        if (!n) {
            snprintf(fallback, sizeof(fallback), "tag %u", t);
            n = fallback;
        }
        printf("    %-24s dirty %+8lld KB (%.1f/cycle)   resident %+8lld KB\n",
               n, d, cycles ? (double)d / cycles : 0.0, r);
    }
    fflush(stdout);
}

static int total_cycles = 40;
static const char *folder = NULL;      /* NULL => do not call set_current_folder */
static gboolean want_filter = TRUE;
static gboolean want_parent = TRUE;
static gboolean want_show = TRUE;
static GtkFileChooserAction action = GTK_FILE_CHOOSER_ACTION_SAVE;
static gboolean want_instances = FALSE;

/* Types worth counting per cycle. Requires GOBJECT_DEBUG=instance-count and a glib
 * built with G_ENABLE_DEBUG.
 *
 * WHEN UNSUPPORTED EVERY COUNT READS 0, which is indistinguishable at the call site
 * from "none of these are live" -- the reading anyone actually wants. This comment
 * used to claim "the mode says so rather than lying" and nothing said so: an
 * unavailable instrument reported zero as a measurement, which is the most
 * expensive shape a probe can have, because a leak hunt reads it as good news.
 * report_instances() now canaries the instrument against a type it is holding live
 * at that instant, and suppresses the whole section if the canary reads 0. */
static const char *tracked_types[] = {
    "GtkFileChooserNative", "GtkFileChooserDialog", "GtkFileChooserWidget",
    "GtkFileSystemModel", "GtkColumnView", "GtkColumnViewCell", "GtkListItemWidget",
    "GtkFilterListModel", "GtkSortListModel", "GtkSingleSelection", "GtkATContext",
    "GtkWindow", "GtkFileFilter", "GLocalFileInfo", "GFileInfo", NULL,
};

/* AppKit side of the same question: how many NSSavePanel/NSOpenPanel objects is
 * the process still holding? GTK retains one per invocation
 * (gtkfilechoosernativequartz.c: [[NSSavePanel savePanel] retain]) and the
 * completion handler only orders it out. */
static gboolean release_accessory = FALSE;
/* An autorelease pool spanning one cycle. AppKit autoreleases heavily while
 * presenting and tearing down a panel; a pure NSApplication drains its pool per 
 * iteration of its own run loop, and the question this mode asks is whether the
 * GLib main loop GTK runs instead leaves those objects undrained. Push/pop is
 * what @autoreleasepool compiles to; spanning two callbacks is legal on one
 * thread provided nothing else pushed a pool in between -- and if GTK did, the
 * pop aborts, which is itself the answer. */
/* Distinguishes "gone from [NSApp windows]" from "actually deallocated". A panel
 * that is merely closed and a panel that is freed look identical to a window-list
 * count, and the whole verdict turns on which one -reap achieves: if the panel
 * really does deallocate and the footprint still does not move, the retained
 * panel is not where the bytes are. */
@interface DeallocSpy : NSObject
@end
static int panel_deallocs = 0;
@implementation DeallocSpy
- (void)dealloc
{
    panel_deallocs++;
    [super dealloc];
}
@end
static char spy_key;
static gboolean track_dealloc = FALSE;
static gboolean release_panel = FALSE;

static void attach_dealloc_spy(void)
{
    for (NSWindow *w in [NSApp windows]) {
        if (![w isKindOfClass:[NSSavePanel class]] || ![w isVisible])
            continue;
        if (objc_getAssociatedObject(w, &spy_key))
            return;
        DeallocSpy *spy = [[DeallocSpy alloc] init];
        objc_setAssociatedObject(w, &spy_key, spy, OBJC_ASSOCIATION_RETAIN);
        [spy release];
        return;
    }
}

static gboolean pool_per_cycle = FALSE;
static void *cycle_pool = NULL;
static int accessories_released = 0;

static void count_panels(int *total, int *visible)
{
    *total = 0;
    *visible = 0;
    for (NSWindow *w in [NSApp windows]) {
        if (![w isKindOfClass:[NSSavePanel class]])
            continue;
        (*total)++;
        if ([w isVisible])
            (*visible)++;
    }
}

/* Candidate remedy, tested here before it is proposed anywhere: close the panels
 * GTK ordered out and never released. Only non-visible panels are touched, so a
 * chooser still on screen is safe.
 *
 * The HYPOTHESIS this probe started from was that -setReleasedWhenClosed:YES is
 * already set on them by GTK, so -close alone balances GTK's retain and frees the
 * view hierarchy. That is REFUTED by what the probe went on to measure -- see the
 * --release-panel comment below: -close drops only AppKit's ownership, GTK's
 * alloc-time retain survives it, and a closed panel does not deallocate. -close on
 * its own is therefore not the remedy; it is the precondition for one. */
/* gtk_window_destroy() returns void, so casting it to GSourceFunc leaves the
 * source's remove-or-repeat verdict to whatever is in the return register --
 * which happens to work until it does not. One-shot, explicitly. */
static gboolean destroy_parent(gpointer p)
{
    gtk_window_destroy(GTK_WINDOW(p));
    return G_SOURCE_REMOVE;
}

static int reap_spent_panels(void)
{
    int reaped = 0;
    NSArray *windows = [[NSApp windows] copy];
    for (NSWindow *w in windows) {
        if (![w isKindOfClass:[NSSavePanel class]] || [w isVisible])
            continue;
        /* --release-accessory emulates the OTHER half of the proposed upstream
         * patch ([data->filter_popup_button release] in
         * filechooser_quartz_data_free) so its effect can be measured before it
         * is proposed. GTK alloc'd the accessory view and never released it; the
         * panel's own retain dies with the panel at -close. Retain across the
         * close so the object survives to be messaged, drop that retain, then
         * drop the one GTK abandoned. If the model is wrong this over-releases
         * and crashes -- which is itself a result, and why it is done here and
         * not in the application. */
        if (release_accessory) {
            id acc = [w accessoryView];
            if (acc && strcmp(class_getName([acc class]), "FilterComboBox") == 0) {
                /* Order matters, and getting it wrong is measurable: the patch
                 * would release the button inside filechooser_quartz_data_free,
                 * while the panel STILL retains it, so the button outlives this
                 * release and dies with the panel. Releasing it after the panel
                 * is gone instead drops the last reference at a different moment
                 * and leaves a window per cycle behind -- measured at +940 KB per
                 * cycle, WORSE than doing nothing. */
                [acc release];  /* balances GTK's unbalanced +1 from alloc */
                accessories_released++;
            }
        }
        [w close];
        if (release_panel) {
            /* -setReleasedWhenClosed:YES means -close dropped APPKIT's ownership;
             * GTK's own alloc-time retain is still outstanding, which is why a
             * closed panel does not deallocate. This balances it -- the effect the
             * proposed [data->panel release] in filechooser_quartz_data_free would
             * have. The DeallocSpy says whether it worked; the window-list count
             * cannot, because a closed-but-live panel has already left the list. */
            [w release];
        }
        reaped++;
    }
    [windows release];
    return reaped;
}

/* Histogram of [NSApp windows] by class. The panel is not the only window an
 * invocation creates, and "NSSavePanel count is 0" can coexist with a window
 * accumulating every cycle -- so name the classes rather than trusting a single
 * counter to speak for the whole window list. */
static void report_window_classes(void)
{
    NSCountedSet *set = [[NSCountedSet alloc] init];
    for (NSWindow *w in [NSApp windows])
        [set addObject:[NSString stringWithUTF8String:class_getName([w class])]];
    printf("    windows by class:");
    for (NSString *name in set)
        printf(" %s=%lu", [name UTF8String], (unsigned long)[set countForObject:name]);
    printf("\n");
    [set release];
}

/* Is g_type_get_instance_count actually counting? Build a GtkLabel, hold it, and ask
 * for GtkLabel's count. A live instance we are holding MUST read >= 1; a 0 there can
 * only mean the instrument is off. Runs once, on the first report. */
static gboolean instance_counting_available(void)
{
    GtkWidget *canary = gtk_label_new("canary");
    g_object_ref_sink(canary);
    int n = g_type_get_instance_count(GTK_TYPE_LABEL);
    g_object_unref(canary);
    return n > 0;
}

static void report_instances(const char *when)
{
    static gboolean canaried = FALSE;
    if (!canaried) {
        canaried = TRUE;
        if (!instance_counting_available()) {
            printf("instance counting: UNAVAILABLE (set GOBJECT_DEBUG=instance-count, and a glib\n");
            printf("                   built with G_ENABLE_DEBUG); --instances suppressed rather\n");
            printf("                   than reporting 0 for every type, which reads as a measurement.\n");
            fflush(stdout);
            want_instances = FALSE;
        }
    }
    if (!want_instances)
        return;

    int panels = 0, vis = 0;
    count_panels(&panels, &vis);
    printf("  --- live GObject instances (%s) ---\n", when);
    printf("    %-24s %d (%d visible), NSApp windows %d\n", "NSSavePanel/NSOpenPanel",
           panels, vis, (int)[[NSApp windows] count]);
    report_window_classes();
    for (int i = 0; tracked_types[i]; i++) {
        GType t = g_type_from_name(tracked_types[i]);
        if (t == 0)
            continue;
        printf("    %-24s %d\n", tracked_types[i], g_type_get_instance_count(t));
    }
    fflush(stdout);
}

static int hold_ms = 700;
static int gap_ms = 250;
static int every = 10;
static int done_cycles = 0;
static GtkWindow *parent;
static GtkFileChooserNative *chooser;
static snapshot base, prev, now;
/* Which cycle `prev` was taken at. The baseline lands after the warmup cycle, so the
 * first bucket is one cycle short of `every` and its divisor is not `every`. */
static int prev_cycle = 0;

static gboolean start_cycle(gpointer data);
static int responses_seen = 0;
static gboolean cancel_mode_appkit = FALSE;
static gboolean reap_panels = FALSE;
static int reaped_total = 0;
static int watchdog_ms = 4000;
/* Stay alive after the last cycle so heap(1)/malloc_history(1) can attach at a
 * known-quiet moment instead of racing the exit. */
static int linger_secs = 0;
/* Build the internal chooser WIDGET tree only -- no native panel at all -- and
 * tear it down correctly. GtkFileChooserNative always constructs one of these
 * (gtk_file_chooser_native_init) and delegates every property to it, so this
 * isolates the cost of the GTK-side half of an invocation from the platform
 * panel's, and it runs identically on every platform. */
static gboolean widget_only = FALSE;
static GtkWidget *dlg_widget = NULL;

/* --cancel asks for the USER's dismissal path. When no visible NSSavePanel is found the
 * cycle silently fell through to gtk_native_dialog_hide() -- a different code path with a
 * different teardown -- while the run header still advertised the requested one. The
 * resulting figure is an unlabelled average over two dismissals, which is not the
 * measurement anyone asked for. Counted, reported, and fatal, like the watchdog below. */
static int cancels_issued = 0;
static int hide_fallbacks = 0;

static gboolean watchdog(gpointer data)
{
    if (GPOINTER_TO_INT(data) == done_cycles && chooser != NULL) {
        printf("  note: cancel: produced no response within %d ms (cycle %d) -- aborting\n", watchdog_ms,
               done_cycles + 1);
        exit(2);
    }
    return G_SOURCE_REMOVE;
}

static gboolean teardown_in_response = TRUE;

/* The teardown window/export.rs performs: destroy inside the response handler,
 * then drop the single external strong ref (NativeDialogHolder's shape). */
static void teardown(void)
{
    if (widget_only) {
        if (dlg_widget) {
            gtk_window_destroy(GTK_WINDOW(dlg_widget));
            dlg_widget = NULL;
        }
        return;
    }
    if (!chooser)
        return;
    gtk_native_dialog_destroy(GTK_NATIVE_DIALOG(chooser));
    g_object_unref(chooser);
    chooser = NULL;
}

static void checkpoint(void);
static gboolean deferred_checkpoint(gpointer data);

static gboolean deferred_checkpoint(gpointer data)
{
    (void)data;
    checkpoint();
    return G_SOURCE_REMOVE;
}

static gboolean next_cycle(gpointer data)
{
    (void)data;
    checkpoint();
    if (done_cycles >= total_cycles)
        return G_SOURCE_REMOVE;
    return start_cycle(NULL);
}


static void on_response(GtkNativeDialog *d, int response, gpointer user_data)
{
    (void)d; (void)response; (void)user_data;
    responses_seen++;
    if (!teardown_in_response)
        return;
    teardown();
    if (reap_panels)
        reaped_total += reap_spent_panels();
    if (pool_per_cycle && cycle_pool) {
        objc_autoreleasePoolPop(cycle_pool);
        cycle_pool = NULL;
    }
    done_cycles++;
    /* NOT here: g_signal_emit holds a ref on the instance for the duration of the
     * emission, so an instance count taken inside the handler reads one dialog
     * still live that is in fact about to finalize. Checkpoint from the main loop. */
    if (done_cycles < total_cycles)
        g_timeout_add(gap_ms, next_cycle, NULL);
    else
        g_timeout_add(gap_ms, (GSourceFunc)deferred_checkpoint, NULL);
}

/* Send -cancel: to the live NSSavePanel/NSOpenPanel, which is exactly what the
 * Cancel button does: it ends the sheet, runs GTK's completion handler, and so
 * emits GTK_RESPONSE_CANCEL through the ordinary response path. */
static gboolean cancel_via_appkit(void)
{
    /* AppKit keeps every panel it has ever built in [NSApp windows], so the FIRST
     * NSSavePanel found is usually a spent one from an earlier cycle -- sending
     * -cancel: to that one silently does nothing and the live panel hangs. Take
     * the visible one. */
    for (NSWindow *w in [NSApp windows]) {
        if (![w isKindOfClass:[NSSavePanel class]] || ![w isVisible])
            continue;
        if (done_cycles == 0)
            printf("  panel: %s visible=%d sheet=%d sheetParent=%s attachedSheetOfParent=%d\n",
                   [[[w class] description] UTF8String], (int)[w isVisible], (int)[w isSheet],
                   [w sheetParent] ? "yes" : "no",
                   (int)([w sheetParent] != nil && [[w sheetParent] attachedSheet] == w));
        NSWindow *host = [w sheetParent];
        if (host)
            [host endSheet:w returnCode:NSModalResponseCancel];
        else
            [(NSSavePanel *)w cancel:nil];
        return TRUE;
    }
    return FALSE;
}

static gboolean finish_cycle(gpointer data)
{
    (void)data;
    if (widget_only) {
        if (want_instances && (done_cycles + 1) % every == 0)
            report_instances("pre-teardown");
        teardown();
        done_cycles++;
        checkpoint();
        if (done_cycles < total_cycles)
            g_timeout_add(gap_ms, next_cycle, NULL);
        return G_SOURCE_REMOVE;
    }
    if (track_dealloc)
        attach_dealloc_spy();
    if (cancel_mode_appkit) {
        if (cancel_via_appkit()) {
            cancels_issued++;
            /* The response handler owns teardown and the next cycle. */
            g_timeout_add(watchdog_ms, watchdog, GINT_TO_POINTER(done_cycles));
            return G_SOURCE_REMOVE;
        }
        hide_fallbacks++;
        printf("  note: no NSSavePanel found to cancel (cycle %d) -- FALLING BACK to\n"
               "        gtk_native_dialog_hide(), which is not the path --cancel requested\n",
               done_cycles + 1);
    }
    /* Dismiss without answering -- gtk_native_dialog_hide()'s own path. */
    gtk_native_dialog_hide(GTK_NATIVE_DIALOG(chooser));
    if (teardown_in_response && chooser == NULL)
        return G_SOURCE_REMOVE;   /* response fired synchronously and tore down */
    if (teardown_in_response && chooser != NULL) {
        /* hide() produced no response: say so, then tear down anyway. */
        if (done_cycles == 1)
            printf("  note: hide() emitted no response signal\n");
    }
    teardown();
    done_cycles++;

    checkpoint();

    if (done_cycles >= total_cycles) {
        take_snapshot(&now);
        report_diff("TOTAL (post-warmup baseline -> end)", &base, &now, total_cycles - 1);
        printf("responses seen: %d of %d cycles\n", responses_seen, done_cycles);
        g_idle_add(destroy_parent, parent);
        return G_SOURCE_REMOVE;
    }
    g_timeout_add(gap_ms, start_cycle, NULL);
    return G_SOURCE_REMOVE;
}

static void checkpoint(void)
{
    /* `done_cycles != prev_cycle` suppresses a DUPLICATE at the boundary. checkpoint() has
     * four call sites and more than one can run for the same cycle, so at every `every`-th
     * cycle a second call reported a bucket spanning ZERO cycles. It was invisible while the
     * label was derived from `every`: both calls printed the same "cycles 0..3" and the
     * repeat read as a formatting quirk. Spanning from `prev_cycle` made it legible as
     * "cycles 3..3", which is what a zero-length measurement actually looks like. */
    if (done_cycles % every == 0 && done_cycles != prev_cycle) {
        take_snapshot(&now);
        if (want_instances) {
            char w[32];
            snprintf(w, sizeof(w), "after %d cycles", done_cycles);
            report_instances(w);
        }
        /* The first bucket is SHORT. The baseline is taken after the warmup cycle, so
         * with every=10 the first checkpoint spans cycles 1..10 -- nine cycles, not ten
         * -- and dividing by `every` under-reported its per-cycle growth by 10% while
         * labelling a range that never happened. Span from where prev was actually taken. */
        char label[64];
        int span = done_cycles - prev_cycle;
        snprintf(label, sizeof(label), "cycles %d..%d", prev_cycle, done_cycles);
        report_diff(label, &prev, &now, span > 0 ? span : 1);
        prev = now;
        prev_cycle = done_cycles;
    }
    if ((teardown_in_response || widget_only) && done_cycles >= total_cycles) {
        take_snapshot(&now);
        report_diff("TOTAL (post-warmup baseline -> end)", &base, &now, total_cycles - 1);
        printf("responses seen: %d of %d cycles, panels reaped: %d, accessory views released: %d, panels DEALLOCATED: %d\n",
               responses_seen, done_cycles, reaped_total, accessories_released, panel_deallocs);
        if (cancel_mode_appkit) {
            printf("dismissal: cancel=%d hide_fallback=%d\n", cancels_issued, hide_fallbacks);
            if (hide_fallbacks > 0) {
                printf("INVALID: %d of %d cycles did not use the dismissal path --cancel asked for,\n"
                       "         so the figures above average two different teardowns.\n",
                       hide_fallbacks, done_cycles);
                fflush(stdout);
                exit(2);
            }
        }
        if (linger_secs > 0) {
            printf("lingering %d s (pid %d) -- attach now\n", linger_secs, (int)getpid());
            fflush(stdout);
            g_timeout_add_seconds(linger_secs, destroy_parent, parent);
        } else {
            g_idle_add(destroy_parent, parent);
        }
    }
}

static gboolean start_cycle(gpointer data)
{
    (void)data;
    /* The first construction in a process pays a large one-time cost (native panel
     * machinery warming up); the baseline is taken AFTER it so it is not counted. */
    if (done_cycles == 1) {
        take_snapshot(&base);
        prev = base;
        prev_cycle = done_cycles;   /* 1: the warmup cycle is already spent */
        printf("baseline after 1 warmup cycle: rss %lld KB, footprint %lld KB, malloc %lld KB\n",
               base.rss_kb, base.footprint_kb, base.malloc_in_use / 1024);
        fflush(stdout);
    }

    if (pool_per_cycle && cycle_pool == NULL)
        cycle_pool = objc_autoreleasePoolPush();

    if (widget_only) {
        dlg_widget = g_object_new(GTK_TYPE_FILE_CHOOSER_DIALOG, NULL);
        gtk_window_set_hide_on_close(GTK_WINDOW(dlg_widget), TRUE);
        gtk_file_chooser_set_action(GTK_FILE_CHOOSER(dlg_widget), action);
        if (want_filter) {
            GtkFileFilter *f = gtk_file_filter_new();
            gtk_file_filter_set_name(f, "PDF files");
            gtk_file_filter_add_pattern(f, "*.pdf");
            gtk_file_chooser_add_filter(GTK_FILE_CHOOSER(dlg_widget), f);
        }
        if (action == GTK_FILE_CHOOSER_ACTION_SAVE)
            gtk_file_chooser_set_current_name(GTK_FILE_CHOOSER(dlg_widget), "probe.pdf");
        if (folder) {
            GFile *d = g_file_new_for_path(folder);
            gtk_file_chooser_set_current_folder(GTK_FILE_CHOOSER(dlg_widget), d, NULL);
            g_object_unref(d);
        }
        g_timeout_add(hold_ms, finish_cycle, NULL);
        return G_SOURCE_REMOVE;
    }

    chooser = gtk_file_chooser_native_new("Export as PDF",
                                          want_parent ? parent : NULL,
                                          action, "Export", "Cancel");
    if (want_filter) {
        GtkFileFilter *filter = gtk_file_filter_new();
        gtk_file_filter_set_name(filter, "PDF files");
        gtk_file_filter_add_pattern(filter, "*.pdf");
        gtk_file_chooser_add_filter(GTK_FILE_CHOOSER(chooser), filter);
    }
    if (action == GTK_FILE_CHOOSER_ACTION_SAVE)
        gtk_file_chooser_set_current_name(GTK_FILE_CHOOSER(chooser), "probe.pdf");
    if (folder) {
        GFile *dir = g_file_new_for_path(folder);
        gtk_file_chooser_set_current_folder(GTK_FILE_CHOOSER(chooser), dir, NULL);
        g_object_unref(dir);
    }

    if (want_show)
        g_signal_connect(chooser, "response", G_CALLBACK(on_response), NULL);
    if (want_show)
        gtk_native_dialog_show(GTK_NATIVE_DIALOG(chooser));
    g_timeout_add(want_show ? hold_ms : 1, finish_cycle, NULL);
    return G_SOURCE_REMOVE;
}

static void on_activate(GtkApplication *app, gpointer user_data)
{
    (void)user_data;
    parent = GTK_WINDOW(gtk_application_window_new(app));
    gtk_window_set_title(parent, "native-chooser-rss probe");
    gtk_window_set_default_size(parent, 640, 400);
    gtk_window_present(parent);
    g_timeout_add(800, start_cycle, NULL);
}

int main(int argc, char **argv)
{
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--cycles") && i + 1 < argc) total_cycles = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--hold") && i + 1 < argc) hold_ms = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--gap") && i + 1 < argc) gap_ms = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--every") && i + 1 < argc) every = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--folder") && i + 1 < argc) folder = argv[++i];
        else if (!strcmp(argv[i], "--no-filter")) want_filter = FALSE;
        else if (!strcmp(argv[i], "--no-parent")) want_parent = FALSE;
        else if (!strcmp(argv[i], "--no-show")) want_show = FALSE;
        else if (!strcmp(argv[i], "--open")) action = GTK_FILE_CHOOSER_ACTION_OPEN;
        else if (!strcmp(argv[i], "--instances")) want_instances = TRUE;
        else if (!strcmp(argv[i], "--teardown-in-timeout")) teardown_in_response = FALSE;
        else if (!strcmp(argv[i], "--cancel")) cancel_mode_appkit = TRUE;
        else if (!strcmp(argv[i], "--reap")) reap_panels = TRUE;
        else if (!strcmp(argv[i], "--pool-per-cycle")) pool_per_cycle = TRUE;
        else if (!strcmp(argv[i], "--track-dealloc")) track_dealloc = TRUE;
        else if (!strcmp(argv[i], "--release-panel")) { reap_panels = TRUE; release_panel = TRUE; track_dealloc = TRUE; }
        else if (!strcmp(argv[i], "--release-accessory")) { reap_panels = TRUE; release_accessory = TRUE; }
        else if (!strcmp(argv[i], "--linger") && i + 1 < argc) linger_secs = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--widget-only")) widget_only = TRUE;
        else if (!strcmp(argv[i], "--watchdog") && i + 1 < argc) watchdog_ms = atoi(argv[++i]);
        else {
            /* Silently ignoring an unknown flag makes a typo look like a measurement of
             * the mode you meant to select: --instaces runs the default configuration and
             * prints a clean, wrong answer under the heading you asked for. */
            fprintf(stderr, "unknown argument: %s\n", argv[i]);
            return 2;
        }
    }
    printf("probe: %d cycles, hold %d ms, gap %d ms, checkpoint every %d\n",
           total_cycles, hold_ms, gap_ms, every);
    printf("       action=%s folder=%s filter=%s parent=%s show=%s\n",
           action == GTK_FILE_CHOOSER_ACTION_SAVE ? "save" : "open",
           folder ? folder : "(unset)", want_filter ? "yes" : "no",
           want_parent ? "yes" : "no", want_show ? "yes" : "no");
    printf("       dismissal=%s\n", cancel_mode_appkit ? "-[NSSavePanel cancel:] (the user's path)"
                                                       : "gtk_native_dialog_hide()");
    /* Every mode that changes what is measured is echoed, not just some of them: a run
     * transcript that omits a mode cannot be told apart from one that did not use it. */
    printf("       widget-only=%s instances=%s reap=%s pool-per-cycle=%s track-dealloc=%s\n",
           widget_only ? "yes" : "no", want_instances ? "yes" : "no",
           reap_panels ? "yes" : "no", pool_per_cycle ? "yes" : "no",
           track_dealloc ? "yes" : "no");
    printf("       release-panel=%s release-accessory=%s watchdog=%d ms linger=%d s\n",
           release_panel ? "yes" : "no", release_accessory ? "yes" : "no",
           watchdog_ms, linger_secs);
    fflush(stdout);

    GtkApplication *app = gtk_application_new("org.scribobulate.probe.chooserrss",
                                              G_APPLICATION_NON_UNIQUE);
    g_signal_connect(app, "activate", G_CALLBACK(on_activate), NULL);
    int status = g_application_run(G_APPLICATION(app), 0, NULL);
    g_object_unref(app);
    return status;
}
