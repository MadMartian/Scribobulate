/*
 * textbuffer-selection-leak.c — does a GtkTextBuffer leak a reference per
 * select-then-deselect, and does taking PRIMARY over change that?
 *
 * SUBJECT: ScrAP-313. `update_selection_clipboards` builds
 * `selection_content = gtk_text_buffer_content_new (buffer)`, which takes `g_object_ref
 * (buffer)`; on deselect it nulls that content's `text_buffer` WITHOUT unreffing and
 * `g_clear_object`s the content, which implements neither finalize nor dispose. The
 * reference is dropped on the floor.
 *
 * WHY MEASURE IT AGAIN. Two reasons, and the second is the one that matters.
 *   1. The entry's refcount table was measured in plain C on another stack; it records a
 *      SOURCE read at 4.22.4, not a RUN. ScrAP-157 is this project's standing example of a
 *      Linux-era defect that is simply absent on this GTK, so "still true here" is a
 *      question rather than an assumption.
 *   2. It was cited against a proposed change with the claim that the application's PRIMARY
 *      take-over AVOIDS the leak, so removing the take-over would reintroduce it. The entry
 *      says the opposite in terms: *"`update_selection_clipboards` builds `selection_content`
 *      whether or not any selection clipboard is registered, so no application change makes
 *      it better or worse. Taking PRIMARY over with a custom provider does not fix it either
 *      — it only avoids ADDING a second strong reference."* This probe settles which reading
 *      the running toolkit supports.
 *
 * ── MEASURED 2026-08-21, GTK 4.22.4 / Quartz, macOS 25 Darwin.0.5 aarch64. ────────────
 *
 *   arm                              fresh  final  delta over 7 cycles
 *   --no-select (CONTROL)              3      3     +0
 *   select/deselect                    3     10     +7
 *   select/deselect --takeover         3     10     +7   <- IDENTICAL
 *
 * 1. THE LEAK REPRODUCES HERE. One reference per select-then-deselect, +1 per cycle, which
 *    is ScrAP-313's recorded rate exactly (its table starts from a fresh count of 1 and
 *    reaches 8 after seven; this rig starts at 3 because the view and window hold refs, and
 *    reaches 10 — the same +7). The control is flat, so the growth is attributable to
 *    selecting rather than to the loop.
 * 2. THE TAKE-OVER DOES NOT AVOID IT. Removing GTK's selection-clipboard registration — what
 *    `wire_primary_selection` does — changes the number not at all. This is what ScrAP-313
 *    already says in terms: `update_selection_clipboards` builds `selection_content` whether
 *    or not any selection clipboard is registered, so no application change makes it better
 *    or worse; a custom provider only avoids ADDING a second strong reference.
 * 3. CONSEQUENCE FOR ANY PROPOSAL TO DELETE THE TAKE-OVER: deleting it does not reintroduce
 *    this leak, because the take-over was never holding it back. Both views leak today, the
 *    one with the take-over and the one without, and neither can stop it from application
 *    code. The leak is orthogonal to the PRIMARY paste question entirely.
 *
 * ⚠ AND THIS IS THE ENTRY THAT WARNS ABOUT CONTAMINATING NEARBY LEAK TESTS. Any "does closing
 * a tab free the document?" check written near a text buffer fails for this reason and reads
 * as a regression caused by whatever change is in flight. The --no-select arm here is that
 * warning obeyed rather than cited.
 *
 * MODES:
 *   --cycles N     select-then-deselect N times, printing the buffer's refcount each time
 *   --no-select    ⚠ THE CONTROL. Same loop, same cycle count, NEVER selects. A refcount
 *                  that also climbs here would mean the loop itself is retaining, and the
 *                  headline number would be measuring the probe. Without this arm, "it grew"
 *                  is not attributable to selecting (ScrAP-322).
 *   --takeover     remove GTK's selection-clipboard registration first, i.e. what
 *                  `wire_primary_selection` does. If the leak is application-avoidable, this
 *                  arm holds the refcount flat. If ScrAP-313 is right, it changes nothing.
 *
 * The refcount is read with GObject's own `ref_count` field — the same instrument the entry
 * used, so the numbers are comparable to its table by construction.
 *
 * BUILD: cc -o /tmp/tbsl probes/textbuffer-selection-leak.c $(pkg-config --cflags --libs gtk4)
 */

#include <gtk/gtk.h>
#include <stdio.h>
#include <string.h>

static int cycles = 7;
static gboolean no_select = FALSE;
static gboolean takeover = FALSE;

static guint refcount_of(gpointer obj) { return ((GObject *)obj)->ref_count; }

static void on_activate(GtkApplication *app, gpointer data)
{
    (void)data;
    GtkWidget *win = gtk_application_window_new(app);
    GtkWidget *view = gtk_text_view_new();
    GtkTextBuffer *buf = gtk_text_buffer_new(NULL);
    gtk_text_buffer_set_text(buf, "the quick brown fox jumps over the lazy dog", -1);
    gtk_text_view_set_buffer(GTK_TEXT_VIEW(view), buf);
    gtk_window_set_child(GTK_WINDOW(win), view);
    gtk_window_present(GTK_WINDOW(win));
    for (int i = 0; i < 200; i++)
        g_main_context_iteration(NULL, FALSE);

    if (takeover) {
        /* Exactly what src/clipboard.rs::wire_primary_selection does at realize. */
        gtk_text_buffer_remove_selection_clipboard(buf, gtk_widget_get_primary_clipboard(view));
        printf("  take-over applied: GTK's selection clipboard removed from the buffer\n");
    }

    guint fresh = refcount_of(buf);
    printf("  fresh refcount: %u\n", fresh);

    for (int c = 1; c <= cycles; c++) {
        if (!no_select) {
            GtkTextIter s, e;
            gtk_text_buffer_get_start_iter(buf, &s);
            gtk_text_buffer_get_end_iter(buf, &e);
            gtk_text_buffer_select_range(buf, &s, &e);
            for (int i = 0; i < 20; i++)
                g_main_context_iteration(NULL, FALSE);
            guint sel = refcount_of(buf);

            /* Deselect by collapsing to the start. */
            GtkTextIter z;
            gtk_text_buffer_get_start_iter(buf, &z);
            gtk_text_buffer_place_cursor(buf, &z);
            for (int i = 0; i < 20; i++)
                g_main_context_iteration(NULL, FALSE);
            printf("  cycle %d: after select %u, after deselect %u\n", c, sel, refcount_of(buf));
        } else {
            /* The control does the same amount of main-loop work and the same number of
             * iterations, and touches the same buffer — it simply never selects. */
            GtkTextIter z;
            gtk_text_buffer_get_start_iter(buf, &z);
            gtk_text_buffer_place_cursor(buf, &z);
            for (int i = 0; i < 40; i++)
                g_main_context_iteration(NULL, FALSE);
            printf("  cycle %d (NO SELECT): refcount %u\n", c, refcount_of(buf));
        }
    }

    guint final = refcount_of(buf);
    printf("  final refcount: %u (fresh was %u, delta %+d over %d cycles)\n", final, fresh,
           (int)final - (int)fresh, cycles);
    if (no_select) {
        if (final != fresh)
            printf("  ⚠ CONTROL MOVED: the loop itself retains; the select arm's number is\n"
                   "    measuring this probe, not the toolkit\n");
        else
            printf("  control flat, as required: growth in the select arm is attributable\n");
    } else if ((int)final - (int)fresh >= cycles) {
        printf("  LEAK REPRODUCED: one reference per select-then-deselect\n");
    } else if (final == fresh) {
        printf("  NO LEAK on this stack — ScrAP-313 does not reproduce here as written\n");
    } else {
        printf("  PARTIAL: grew, but not one per cycle; the mechanism is not what was recorded\n");
    }

    fflush(stdout);
    gtk_window_destroy(GTK_WINDOW(win));
}

int main(int argc, char **argv)
{
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--cycles") && i + 1 < argc)
            cycles = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--no-select"))
            no_select = TRUE;
        else if (!strcmp(argv[i], "--takeover"))
            takeover = TRUE;
        else {
            fprintf(stderr, "unknown argument: %s\n", argv[i]);
            return 2;
        }
    }
    printf("mode: %s%s, %d cycles\n", no_select ? "--no-select (CONTROL)" : "select/deselect",
           takeover ? " +takeover" : "", cycles);
    fflush(stdout);

    GtkApplication *app =
        gtk_application_new("org.scribobulate.probe.tbsl", G_APPLICATION_NON_UNIQUE);
    g_signal_connect(app, "activate", G_CALLBACK(on_activate), NULL);
    int status = g_application_run(G_APPLICATION(app), 0, NULL);
    g_object_unref(app);
    return status;
}
