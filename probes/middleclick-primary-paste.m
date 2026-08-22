/*
 * middleclick-primary-paste.m — can an application replace GTK's middle-click PRIMARY paste
 * with its own, and does its gesture actually receive the event?
 *
 * SUBJECT: `gtk-enable-primary-paste` (a per-display GtkSettings property), the GtkTextView
 * middle-click branch it gates, and whether a custom GtkGestureClick sees button 2 once that
 * branch is not taken. GTK 4.22.4, macOS Quartz.
 *
 * WHY. `src/clipboard.rs::wire_primary_selection` takes PRIMARY over from GTK so a
 * same-application middle-click paste arrives as ONE insert-text emission rather than one per
 * syntax-tag toggle (ScrAP-312, which is where CRLF corruption comes from). Two designs for
 * that have been measured to destruction in this directory:
 *   - REMOVE GTK's selection-clipboard registration: ref-counted, undone by every
 *     `set_buffer`, and makes GTK's own removal raise from inside set_buffer
 *     (`textview-selection-clipboard.c`).
 *   - OVERWRITE the clipboard content: `gtk_text_buffer_content_detach` collapses the
 *     selection the moment the provider is displaced (`textview-primary-overwrite.c`).
 * Both attacked the PUBLISHER in order to change what the CONSUMER does. This probe tests
 * attacking the consumer instead: leave GTK's publishing entirely alone, turn its middle-click
 * paste off, and supply our own that reads text/plain.
 *
 * ── ARMS. A 2x2, agreed BEFORE running, so no claim here is uncontrolled. ───────────────
 *
 *   --baseline   setting ON,  no gesture   -> GTK pastes.  PROVES THE RIG DELIVERS A MIDDLE
 *                CLICK AT ALL. Run this FIRST and stop if it fails: without it, a silent
 *                gesture in --ours is indistinguishable from an undelivered event, and you
 *                would debug a gesture against a dead rig.
 *   --gtk-off    setting OFF, no gesture   -> nothing pastes. Proves the SETTING is what
 *                gates GTK's branch, rather than something else about the arm.
 *   --ours       setting OFF, gesture      -> our handler runs and inserts plain text.
 *   --both-on    setting ON,  gesture      -> who runs? Decides whether turning the setting
 *                off is REQUIRED or merely tidy. GTK claims the sequence before pasting
 *                (gtktextview.c:5867), so PHASE is a variable here and is recorded; pass
 *                --capture to run the gesture in the capture phase instead of bubble.
 *
 * ── GESTURE LIVENESS. The control for the gesture ITSELF. ──────────────────────────────
 * The gesture logs EVERY button it sees, with the button number, not just button 2. Without
 * that, a failing --ours cannot distinguish "the event was never delivered" from "my gesture
 * is misconfigured" — wrong button, wrong phase, wrong widget — and those are different
 * subjects with identical symptoms. This is the arm whose absence produced two retractions
 * in `textview-primary-overwrite.c`: an instrument that is never itself under test.
 *
 * ── MEASURED 2026-08-21, GTK 4.22.4 / Quartz, unlocked session. THE DESIGN WORKS. ─────
 *
 *   arm                  setting  gesture  GTK pasted  ours ran  bytes
 *   --baseline           ON       no       yes         0         MATCH
 *   --gtk-off            OFF      no       no          0         (nothing)
 *   --ours               OFF      yes      no          1         MATCH
 *   --both-on (bubble)   ON       yes      yes         1         DIFFER (pasted twice)
 *   --both-on (capture)  ON       yes      yes         1         DIFFER (pasted twice)
 *
 * (1) OUR GESTURE RECEIVES THE MIDDLE CLICK once GTK's branch is not taken, and the text it
 *     inserts is BYTE-IDENTICAL to the source — the requirement, not merely "a paste
 *     happened". That was the one unverified risk in the design and it is now measured.
 * (2) LEAVING THE SETTING ON AND NOT CLAIMING GIVES A DOUBLE PASTE — GTK and our handler
 *     both run. So SOMETHING must suppress GTK; (4) shows a claim is the cheaper something.
 * (3) PHASE DOES NOT DECIDE WHETHER OUR HANDLER RUNS — it runs in both — BUT IT DECIDES
 *     WHETHER WE CAN DENY GTK. See (4); recording phase turned out to be load-bearing rather
 *     than bookkeeping.
 *
 * ── (4) THE GLOBAL SETTING DOES NOT HAVE TO BE TOUCHED AT ALL. ─────────────────────────
 *
 * `gtk-enable-primary-paste` is PER-DISPLAY, so turning it off removes middle-click paste
 * from every GtkEntry in the process — and middle-click paste is a real platform convention
 * on X11, so that is a user-visible cost paid across ~7 production entry sites to fix one
 * view. Claiming the sequence avoids it entirely:
 *
 *   setting  phase    claim  GTK pasted  ours ran  bytes
 *   ON       capture  YES    NO          1         MATCH      <- the design
 *   ON       bubble   yes    yes         1         DIFFER (twice)
 *   ON       capture  no     yes         1         DIFFER (twice)
 *   ON       bubble   no     yes         1         DIFFER (twice)
 *
 * A CAPTURE-phase gesture that calls `gtk_gesture_set_state(..., GTK_EVENT_SEQUENCE_CLAIMED)`
 * DENIES GTK's own middle-click branch; the same claim in BUBBLE is too late, because GTK's
 * handler has already run. So the change stays local to the one view that needs it, the
 * global setting is left alone, and every other text widget keeps GTK's behaviour.
 *
 * ⚠ TWO RIG DEFECTS FOUND BEFORE ANY OF THE ABOVE COULD BE TRUSTED, both of which produced a
 * confident, wrong NEGATIVE:
 *   - The click was posted with CGEventPostToPid to our own process and pumped by hand with
 *     `g_main_context_iteration(NULL, FALSE)`. Nothing was ever delivered: window key,
 *     coordinates inside it, no paste. It takes the HID tap and the application actually
 *     RUNNING for GTK's Quartz integration to see the event.
 *   - The button number was computed as `click_button - 1`. macOS numbers are 0=left,
 *     1=RIGHT, 2=centre — not X11's 1/2/3 shifted down — so every "middle" click was a RIGHT
 *     click. GTK reported button 3 and the arm read as "the gesture fired but our paste did
 *     not run", which is a DESIGN conclusion drawn from an instrument defect.
 * Both were caught by controls rather than by inspection: --baseline refused to pass while
 * the rig was dead, and the liveness log printed the button NUMBER instead of only reacting
 * to button 2. Neither would have been visible in a probe that reported only its verdict.
 *
 * INPUT is a real button-2 press posted with CGEventPostToPid to this process, at a point
 * inside our own NSWindow. It therefore requires an UNLOCKED session; a locked one cannot
 * receive it, which is a feasibility limit and not a validity one.
 *
 * BUILD: cc -o /tmp/mcpp probes/middleclick-primary-paste.m \
 *          $(pkg-config --cflags --libs gtk4) -framework Cocoa -framework CoreGraphics
 */

#import <Cocoa/Cocoa.h>
#include <gtk/gtk.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

typedef enum { ARM_BASELINE, ARM_GTK_OFF, ARM_OURS, ARM_BOTH_ON } Arm;
static Arm arm = ARM_BASELINE;
static gboolean capture_phase = FALSE;
static gboolean claim_sequence = FALSE; /* --claim: deny GTK by claiming the sequence */
static int click_button = 2; /* --left posts button 1 instead, to test the RIG */

static GtkWidget *source_view = NULL;
static GtkWidget *target_view = NULL;
static GtkTextBuffer *target_buf = NULL;

static int buttons_seen[8] = {0};
static int our_paste_ran = 0;

static const char *SOURCE_TEXT = "alpha\r\nbeta gamma";

/* ── our replacement for GTK's middle-click paste ─────────────────────────────────────── */
static void on_text_read(GObject *src, GAsyncResult *res, gpointer data)
{
    (void)data;
    GError *err = NULL;
    char *text = gdk_clipboard_read_text_finish(GDK_CLIPBOARD(src), res, &err);
    if (!text) {
        printf("  our handler: read_text failed: %s\n", err ? err->message : "(no error)");
        g_clear_error(&err);
        return;
    }
    GtkTextIter end;
    gtk_text_buffer_get_end_iter(target_buf, &end);
    gtk_text_buffer_insert(target_buf, &end, text, -1);
    our_paste_ran++;
    printf("  our handler: inserted %zu bytes as PLAIN TEXT\n", strlen(text));
    g_free(text);
}

static void on_pressed(GtkGestureClick *g, int n_press, double x, double y, gpointer data)
{
    (void)n_press;
    (void)x;
    (void)y;
    (void)data;
    guint b = gtk_gesture_single_get_current_button(GTK_GESTURE_SINGLE(g));
    if (b < 8)
        buttons_seen[b]++;
    /* LIVENESS: every button, every time. A gesture that logs only button 2 cannot tell
     * "not delivered" from "misconfigured". */
    printf("  gesture saw button %u (phase=%s)\n", b, capture_phase ? "capture" : "bubble");
    if (b != 2)
        return;
    if (claim_sequence) {
        /* Does CLAIMING deny GTK's own middle-click branch? It matters commercially, not just
         * tidily: `gtk-enable-primary-paste` is PER-DISPLAY, so turning it off removes
         * middle-click paste from every GtkEntry in the process — and middle-click paste is a
         * real platform convention on X11. If a capture-phase claim suppresses GTK's branch,
         * the global setting never has to be touched and the change stays local to one view. */
        gtk_gesture_set_state(GTK_GESTURE(g), GTK_EVENT_SEQUENCE_CLAIMED);
        printf("  gesture CLAIMED the sequence\n");
    }
    GdkClipboard *primary = gtk_widget_get_primary_clipboard(target_view);
    gdk_clipboard_read_text_async(primary, NULL, on_text_read, NULL);
}

/* ── deliver a real middle click into our own window ──────────────────────────────────── */
static void post_middle_click(void)
{
    [NSApp activateIgnoringOtherApps:YES];
    NSWindow *win = nil;
    for (NSWindow *w in [NSApp windows]) {
        if ([w isVisible]) {
            win = w;
            break;
        }
    }
    if (!win) {
        printf("  RIG FAILURE: no visible NSWindow to click into\n");
        return;
    }
    NSRect f = [win frame];
    printf("  NSWindow frame = (%.0f,%.0f) %.0fx%.0f  key=%d main=%d  windows=%lu\n",
           f.origin.x, f.origin.y, f.size.width, f.size.height, (int)[win isKeyWindow],
           (int)[win isMainWindow], (unsigned long)[[NSApp windows] count]);
    /* Lower half of the window is the target view; CGEvent uses a top-left origin, AppKit a
     * bottom-left one, so the y flip is against the whole screen height. */
    CGFloat sx = f.origin.x + f.size.width / 2.0;
    CGFloat sy_appkit = f.origin.y + f.size.height * 0.25;
    CGFloat screen_h = [[[NSScreen screens] objectAtIndex:0] frame].size.height;
    CGPoint p = CGPointMake(sx, screen_h - sy_appkit);
    printf("  posting button-%d click at (%.0f, %.0f)\n", click_button, p.x, p.y);

    /* Move the pointer there first: a click at a location the system has never seen the
     * cursor at is not the same event sequence a user produces. */
    CGEventRef move = CGEventCreateMouseEvent(NULL, kCGEventMouseMoved, p, kCGMouseButtonLeft);
    CGEventPost(kCGHIDEventTap, move);
    CFRelease(move);
    g_usleep(120000);

    CGEventType tdown = click_button == 1 ? kCGEventLeftMouseDown : kCGEventOtherMouseDown;
    CGEventType tup = click_button == 1 ? kCGEventLeftMouseUp : kCGEventOtherMouseUp;
    CGMouseButton b = click_button == 1 ? kCGMouseButtonLeft : kCGMouseButtonCenter;

    CGEventRef down = CGEventCreateMouseEvent(NULL, tdown, p, b);
    CGEventRef up = CGEventCreateMouseEvent(NULL, tup, p, b);
    /* macOS button NUMBERS are 0=left, 1=RIGHT, 2=centre — they are not X11's 1/2/3 shifted
     * by one. An earlier version wrote `click_button - 1`, which posted button number 1 for a
     * "middle" click, i.e. a RIGHT click, and GTK duly reported button 3. The liveness log is
     * the only reason that was visible: without the button NUMBER in the output the arm read
     * as "the gesture fired but our paste did not run", which is a design conclusion drawn
     * from an instrument defect. */
    int mac_button = (click_button == 1) ? 0 : 2;
    CGEventSetIntegerValueField(down, kCGMouseEventButtonNumber, mac_button);
    CGEventSetIntegerValueField(up, kCGMouseEventButtonNumber, mac_button);
    /* kCGHIDEventTap, not CGEventPostToPid: posting to our own pid put the event on the
     * process queue and GTK's Quartz integration never picked it up. The HID tap goes through
     * the normal input stack to the frontmost application, which is us after
     * activateIgnoringOtherApps. Requires Accessibility trust — AXIsProcessTrusted() is 1
     * here, checked rather than assumed. */
    CGEventPost(kCGHIDEventTap, down);
    g_usleep(60000);
    CGEventPost(kCGHIDEventTap, up);
    CFRelease(down);
    CFRelease(up);
}

static void pump(int n)
{
    /* GTK's Quartz backend feeds NSEvents into the GLib main context itself, so iterating
     * the context is the whole pump. An earlier version also peeked at NSApp's queue with
     * `dequeue:NO`, which cannot deliver anything and only risked interfering. */
    for (int i = 0; i < n; i++) {
        g_main_context_iteration(NULL, FALSE);
        g_usleep(2000);
    }
}

static gboolean click_cb(gpointer data)
{
    (void)data;
    post_middle_click();
    return G_SOURCE_REMOVE;
}

static gboolean report_cb(gpointer data)
{
    GtkWidget *win = GTK_WIDGET(data);
    GtkTextIter ts, te;
    gtk_text_buffer_get_bounds(target_buf, &ts, &te);
    char *got = gtk_text_buffer_get_text(target_buf, &ts, &te, FALSE);
    printf("  buttons seen by our gesture: 1=%d 2=%d 3=%d\n", buttons_seen[1], buttons_seen[2],
           buttons_seen[3]);
    printf("  our handler ran: %d\n", our_paste_ran);
    printf("  TARGET BUFFER: \"%s\" (%zu bytes)\n", got, strlen(got));

    /* (4) ASSERT THE OUTCOME, NOT THAT A PASTE HAPPENED. A paste that mangles the bytes
     * passes a "something arrived" check and fails the actual requirement. */
    if (strlen(got) == 0)
        printf("  RESULT: NOTHING PASTED\n");
    else if (!strcmp(got, SOURCE_TEXT))
        printf("  RESULT: pasted and BYTES MATCH the source exactly\n");
    else
        printf("  RESULT: pasted but BYTES DIFFER from the source\n");
    g_free(got);
    fflush(stdout);
    gtk_window_destroy(GTK_WINDOW(win));
    return G_SOURCE_REMOVE;
}

static void on_activate(GtkApplication *app, gpointer data)
{
    (void)data;
    gboolean want_setting = (arm == ARM_BASELINE || arm == ARM_BOTH_ON);
    gboolean want_gesture = (arm == ARM_OURS || arm == ARM_BOTH_ON);

    GtkSettings *st = gtk_settings_get_default();
    g_object_set(st, "gtk-enable-primary-paste", want_setting, NULL);
    gboolean readback = FALSE;
    g_object_get(st, "gtk-enable-primary-paste", &readback, NULL);
    printf("  gtk-enable-primary-paste = %s (readback %s)\n", want_setting ? "TRUE" : "FALSE",
           readback ? "TRUE" : "FALSE");

    GtkWidget *win = gtk_application_window_new(app);
    gtk_window_set_default_size(GTK_WINDOW(win), 480, 320);
    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);

    source_view = gtk_text_view_new();
    GtkTextBuffer *sbuf = gtk_text_view_get_buffer(GTK_TEXT_VIEW(source_view));
    gtk_text_buffer_set_text(sbuf, SOURCE_TEXT, -1);
    gtk_widget_set_vexpand(source_view, TRUE);

    target_view = gtk_text_view_new();
    target_buf = gtk_text_view_get_buffer(GTK_TEXT_VIEW(target_view));
    gtk_text_buffer_set_text(target_buf, "", -1);
    gtk_widget_set_vexpand(target_view, TRUE);

    gtk_box_append(GTK_BOX(box), source_view);
    gtk_box_append(GTK_BOX(box), target_view);
    gtk_window_set_child(GTK_WINDOW(win), box);
    gtk_window_present(GTK_WINDOW(win));
    pump(100);

    if (want_gesture) {
        GtkGesture *g = gtk_gesture_click_new();
        gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(g), 0); /* 0 = every button */
        gtk_event_controller_set_propagation_phase(
            GTK_EVENT_CONTROLLER(g), capture_phase ? GTK_PHASE_CAPTURE : GTK_PHASE_BUBBLE);
        g_signal_connect(g, "pressed", G_CALLBACK(on_pressed), NULL);
        gtk_widget_add_controller(target_view, GTK_EVENT_CONTROLLER(g));
        printf("  custom gesture installed (phase=%s, all buttons)\n",
               capture_phase ? "capture" : "bubble");
    }

    /* Publish PRIMARY the way a real selection does: select in the SOURCE view and let GTK
     * claim it. This is the same-application case ScrAP-312 is about — a text/plain read is
     * what we want the consumer to do instead of GTK's GTK_TYPE_TEXT_BUFFER request. */
    GtkTextIter s, e;
    gtk_text_buffer_get_start_iter(sbuf, &s);
    gtk_text_buffer_get_end_iter(sbuf, &e);
    gtk_text_buffer_select_range(sbuf, &s, &e);
    pump(100);
    printf("  PRIMARY owner after selecting in the source view: %s\n",
           gdk_clipboard_get_content(gtk_widget_get_primary_clipboard(source_view)) ? "set"
                                                                                    : "none");

    /* Hand back to the REAL main loop for both the click and the settle. An earlier version
     * did the whole sequence inline with a hand-rolled pump of
     * `g_main_context_iteration(NULL, FALSE)`, and NOTHING WAS EVER DELIVERED: the window was
     * key, the coordinates landed inside it, and the paste never happened. CGEventPostToPid
     * puts the event on the process's queue, and it takes GTK's own Quartz run-loop
     * integration — i.e. the application actually running — to pick it up. That was a rig
     * failure masquerading as a negative result, which is precisely what --baseline exists to
     * catch, and it caught it. */
    g_timeout_add(600, (GSourceFunc)click_cb, NULL);
    g_timeout_add(2200, (GSourceFunc)report_cb, win);
}

int main(int argc, char **argv)
{
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--baseline"))
            arm = ARM_BASELINE;
        else if (!strcmp(argv[i], "--gtk-off"))
            arm = ARM_GTK_OFF;
        else if (!strcmp(argv[i], "--ours"))
            arm = ARM_OURS;
        else if (!strcmp(argv[i], "--both-on"))
            arm = ARM_BOTH_ON;
        else if (!strcmp(argv[i], "--capture"))
            capture_phase = TRUE;
        else if (!strcmp(argv[i], "--left"))
            click_button = 1;
        else if (!strcmp(argv[i], "--claim"))
            claim_sequence = TRUE;
        else {
            fprintf(stderr, "unknown argument: %s\n", argv[i]);
            return 2;
        }
    }
    printf("arm: %s%s\n",
           arm == ARM_BASELINE ? "--baseline (setting ON, no gesture)"
           : arm == ARM_GTK_OFF ? "--gtk-off (setting OFF, no gesture)"
           : arm == ARM_OURS    ? "--ours (setting OFF, gesture)"
                                : "--both-on (setting ON, gesture)",
           capture_phase ? " +capture" : "");
    fflush(stdout);

    GtkApplication *app =
        gtk_application_new("org.scribobulate.probe.mcpp", G_APPLICATION_NON_UNIQUE);
    g_signal_connect(app, "activate", G_CALLBACK(on_activate), NULL);
    int status = g_application_run(G_APPLICATION(app), 0, NULL);
    g_object_unref(app);
    return status;
}
