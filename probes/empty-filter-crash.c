/*
 * empty-filter-crash.c -- does a GtkFileFilter with a NAME but NO RULES abort the
 * Quartz chooser?
 *
 * The chain that predicts it is sound and was verified from source, step by step:
 * _gtk_file_filter_get_as_pattern_nsstrings() returns a non-nil, ZERO-ELEMENT
 * NSArray for a filter with no rules (its only NULL return is a MIME->UTI
 * failure); file_filter_to_quartz() guards NULL and lets empty through;
 * the filter handler's `containsObject:@""` test is NO for an empty array, so it
 * reaches -[NSSavePanel setAllowedFileTypes:] with a non-nil empty array; and
 * Apple's SDK header for that property says, verbatim, "If the array is not nil
 * and the array contains no items, an exception will be raised." GTK primes the
 * initial selection itself at launch, so no user interaction is needed to get
 * there.
 *
 * It does not abort. Measured on GTK 4.22.4 / Quartz, macOS 26.6.1: the chooser
 * shows and dismisses, exit code 0.
 *
 * The probe is kept because of WHY the prediction failed, which is worth more
 * than the prediction: every step was correct except the last, and the last cited
 * a documented CONTRACT as evidence of RUNTIME BEHAVIOUR. See
 * allowed-file-types-empty.m, which tests that premise directly and is the run
 * that turned "my repro didn't crash" into "the premise is false here". Those are
 * different claims and only the second is worth sending anywhere.
 *
 * Build:
 *   clang -O1 -g -Wno-deprecated-declarations -o /tmp/empty-filter-crash \
 *       probes/empty-filter-crash.c $(pkg-config --cflags --libs gtk4)
 */
#include <gtk/gtk.h>
#include <stdio.h>
#include <stdlib.h>

static gboolean quit_cb(gpointer d)
{
    (void)d;
    printf("NO CRASH: chooser shown and dismissed\n");
    exit(0);
}

static void on_activate(GtkApplication *app, gpointer u)
{
    (void)u;
    GtkWindow *win = GTK_WINDOW(gtk_application_window_new(app));
    gtk_window_present(win);

    GtkFileChooserNative *ch = gtk_file_chooser_native_new(
        "Empty filter probe", win, GTK_FILE_CHOOSER_ACTION_SAVE, "Save", "Cancel");

    /* A filter with a name and nothing else: no pattern, no MIME type, no pixbuf
     * formats. This is trivially reachable from public GTK API. */
    GtkFileFilter *f = gtk_file_filter_new();
    gtk_file_filter_set_name(f, "Named but ruleless");
    gtk_file_chooser_add_filter(GTK_FILE_CHOOSER(ch), f);

    printf("showing chooser with a ruleless filter...\n");
    fflush(stdout);
    gtk_native_dialog_show(GTK_NATIVE_DIALOG(ch));
    printf("show() returned without aborting\n");
    fflush(stdout);

    g_timeout_add(1500, quit_cb, NULL);
}

int main(void)
{
    GtkApplication *app = gtk_application_new("org.scribobulate.probe.emptyfilter",
                                              G_APPLICATION_NON_UNIQUE);
    g_signal_connect(app, "activate", G_CALLBACK(on_activate), NULL);
    int status = g_application_run(G_APPLICATION(app), 0, NULL);
    g_object_unref(app);
    return status;
}
