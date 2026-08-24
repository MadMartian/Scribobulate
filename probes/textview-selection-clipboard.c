/*
 * textview-selection-clipboard.c — who actually raises
 * `gtk_text_buffer_remove_selection_clipboard: assertion 'selection_clipboard != NULL'`?
 *
 * SUBJECT: the ref-counted selection-clipboard registration on GtkTextBuffer, and which of
 * its three callers trips the assertion when an application takes PRIMARY over by removing
 * GTK's registration at realize (`src/clipboard.rs::wire_primary_selection`).
 *
 * WHY A PROBE AND NOT A READING. The assertion message names neither the caller nor the
 * buffer. `gtk_text_buffer_remove_selection_clipboard` asserts on the result of
 * `find_selection_clipboard`, a LOCAL, so "this buffer has no registration" is all it says —
 * and there are three live callers (gtktextview.c:2367 in set_buffer's outgoing half,
 * gtktextview.c:5262 in unrealize, and the application's own handler). Reading the message as
 * "MY handler ran and found nothing" is an inference, and the researcher's source trace says
 * that inference is false: "realize" is G_SIGNAL_RUN_FIRST (gtkwidget.c:1781-1788), so GTK's
 * add at gtktextview.c:5243 genuinely precedes a `g_signal_connect` handler.
 *
 * THE PREDICTION UNDER TEST (researcher, INFERRED from 4.22.4 source):
 *   1. realize          -> GTK adds to buffer A (ref 1); the app removes (ref 0, gone). Silent.
 *   2. set_buffer(B)    -> GTK removes from A (gtktextview.c:2367) and finds NOTHING -> CRITICAL
 *                       -> GTK then adds to B (:2428), so B carries GTK's registration.
 *   3. consequence      -> PRIMARY stays with GTK; the take-over is undone by the swap.
 * MEASURED 2026-08-21, GTK 4.22.4 / Quartz, macOS 25 Darwin.0.5 aarch64. THE PREDICTION
 * HOLDS, and the backtrace names the caller outright:
 *
 *     5  libglib   g_return_if_fail_warning
 *     6  libgtk-4  gtk_text_view_set_buffer + 480      <-- the caller
 *     7  tvsc      on_activate
 *
 * --take-over  -> 1 critical, raised from inside set_buffer
 * --control    -> 0 criticals, identical swap
 *
 * So the critical is raised by GTK's own bookkeeping reacting to the application's earlier
 * removal — one step removed from where it appears to come from. An application reading the
 * message as "my handler found nothing" is reading it about the wrong caller, which is the
 * whole reason this probe exists: the assertion is on a LOCAL and names nobody.
 *
 * CONSEQUENCE FOR THE APPLICATION. Removing GTK's registration is not a stable take-over.
 * `gtk_text_view_set_buffer` removes from the outgoing buffer (gtktextview.c:2364-2368) and
 * adds to the incoming one (:2425-2429), both gated only on the view being realized, and the
 * registration is REF-COUNTED rather than idempotent — so add-then-remove goes 1 -> 2 -> 1
 * and leaves GTK registered. Any design built on removal must therefore also own every
 * buffer swap, and still cannot survive GTK's own unbalanced removal. Overwriting the
 * clipboard CONTENT instead leaves GTK's counting intact; GTK guards both of its clears on
 * the content still being its own (gtktextbuffer.c:3934, :4022), so it will not clobber a
 * foreign provider.
 *
 * MODES, and the second is the control:
 *   --take-over   remove GTK's registration after realize, then swap the buffer (expect critical)
 *   --control     identical, WITHOUT the removal (expect silence)
 * A critical in BOTH modes would mean the swap alone raises it and the take-over is
 * irrelevant; silence in both would mean the probe never reached the path and proves nothing.
 *
 * BUILD: cc -o /tmp/tvsc probes/textview-selection-clipboard.c $(pkg-config --cflags --libs gtk4)
 * RUN:   /tmp/tvsc --take-over ; /tmp/tvsc --control
 * BACKTRACE (the point of it):
 *   G_DEBUG=fatal-criticals lldb --batch -o run -o bt -o quit -- /tmp/tvsc --take-over
 */

#include <gtk/gtk.h>
#include <stdio.h>
#include <string.h>
#include <execinfo.h>

static gboolean take_over = TRUE;
static int criticals = 0;

static GLogWriterOutput log_writer(GLogLevelFlags level, const GLogField *fields,
                                   gsize n_fields, gpointer user_data)
{
    (void)user_data;
    const char *msg = NULL;
    for (gsize i = 0; i < n_fields; i++) {
        if (!strcmp(fields[i].key, "MESSAGE"))
            msg = fields[i].value;
    }
    if ((level & G_LOG_LEVEL_CRITICAL) && msg && strstr(msg, "selection_clipboard")) {
        criticals++;
        printf("  CRITICAL SEEN: %s\n", msg);
        /* IN-PROCESS backtrace, because lldb cannot unwind through the stripped Homebrew
         * glib on this machine — `bt` yields frame #0 and stops. The assertion message
         * names neither caller nor buffer, and naming the caller is the whole question, so
         * the instrument has to come from inside. */
        printf("  --- backtrace at the critical ---\n");
        fflush(stdout);
        void *bt[24];
        int n = backtrace(bt, 24);
        backtrace_symbols_fd(bt, n, 1);
        printf("  --- end backtrace ---\n");
        fflush(stdout);
    }
    return g_log_writer_default(level, fields, n_fields, user_data);
}

static void on_activate(GtkApplication *app, gpointer data)
{
    (void)data;
    GtkWidget *win = gtk_application_window_new(app);
    GtkWidget *view = gtk_text_view_new();

    /* The buffer MUST be set before realize: gtk_text_view_realize reads priv->buffer
     * directly (gtktextview.c:5240) rather than through the lazily-creating getter, so a
     * view realized with no buffer never gets an add at all — a second, independent way to
     * reach the same assertion, and one this probe deliberately rules out. */
    GtkTextBuffer *a = gtk_text_buffer_new(NULL);
    gtk_text_buffer_set_text(a, "buffer A", -1);
    gtk_text_view_set_buffer(GTK_TEXT_VIEW(view), a);

    gtk_window_set_child(GTK_WINDOW(win), view);
    gtk_window_present(GTK_WINDOW(win));

    /* Let realize actually happen; the add we depend on is inside it. */
    for (int i = 0; i < 200; i++)
        g_main_context_iteration(NULL, FALSE);

    printf("  realized=%d\n", gtk_widget_get_realized(view));

    if (take_over) {
        /* Exactly what wire_primary_selection does at realize. */
        printf("  removing GTK's registration from buffer A (the take-over)\n");
        fflush(stdout);
        gtk_text_buffer_remove_selection_clipboard(a, gtk_widget_get_primary_clipboard(view));
    } else {
        printf("  control: leaving GTK's registration on buffer A\n");
        fflush(stdout);
    }

    /* The swap the preview performs on every re-render. Predicted trigger. */
    printf("  set_buffer(B) ...\n");
    fflush(stdout);
    GtkTextBuffer *b = gtk_text_buffer_new(NULL);
    gtk_text_buffer_set_text(b, "buffer B", -1);
    gtk_text_view_set_buffer(GTK_TEXT_VIEW(view), b);

    for (int i = 0; i < 100; i++)
        g_main_context_iteration(NULL, FALSE);

    printf("  criticals mentioning selection_clipboard: %d\n", criticals);
    if (take_over && criticals == 0)
        printf("  UNEXPECTED: the take-over did not produce one; the prediction is wrong\n");
    if (!take_over && criticals > 0)
        printf("  UNEXPECTED: the CONTROL produced one, so the swap alone raises it\n");
    fflush(stdout);

    g_object_unref(a);
    g_object_unref(b);
    gtk_window_destroy(GTK_WINDOW(win));
}

int main(int argc, char **argv)
{
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--take-over"))
            take_over = TRUE;
        else if (!strcmp(argv[i], "--control"))
            take_over = FALSE;
        else {
            fprintf(stderr, "unknown argument: %s\n", argv[i]);
            return 2;
        }
    }
    printf("mode: %s\n", take_over ? "--take-over (remove GTK's registration)" : "--control");
    fflush(stdout);

    g_log_set_writer_func(log_writer, NULL, NULL);

    GtkApplication *app =
        gtk_application_new("org.scribobulate.probe.tvsc", G_APPLICATION_NON_UNIQUE);
    g_signal_connect(app, "activate", G_CALLBACK(on_activate), NULL);
    int status = g_application_run(G_APPLICATION(app), 0, NULL);
    g_object_unref(app);
    return status;
}
