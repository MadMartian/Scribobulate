/*
 * textview-primary-overwrite.c — can an application own PRIMARY by OVERWRITING the
 * clipboard content instead of removing GTK's selection-clipboard registration?
 *
 * SUBJECT: the interaction between `gtk_text_buffer_update_selection_clipboards` and a
 * foreign content provider on the primary clipboard, GTK 4.22.4.
 *
 * WHY. `src/clipboard.rs::wire_primary_selection` currently takes PRIMARY over by REMOVING
 * GTK's registration at realize. `textview-selection-clipboard.c` measured what that costs:
 * the registration is ref-counted, `gtk_text_view_set_buffer` removes from the outgoing
 * buffer and adds to the incoming one on every swap, and the application's removal makes
 * GTK's own removal raise `selection_clipboard != NULL` from inside set_buffer. The preview
 * swaps its buffer on every re-render, so that design cannot hold there.
 *
 * The proposed alternative (researcher, from the 4.22.4 source) is to leave GTK's counting
 * alone and simply set the clipboard's CONTENT afterwards. Three claims decide whether it
 * works, and this probe measures each rather than trusting the reading:
 *
 *   1. HOW OFTEN does GTK claim? `update_selection_clipboards` sets content
 *      UNCONDITIONALLY (gtktextbuffer.c:3922) — no check on who owns it — and has two call
 *      sites, the "mark-set" class closure (:3053) and the delete path (:2080). Predicted:
 *      TWO qualifying emissions per user-visible selection change, because a selection moves
 *      BOTH `insert` and `selection_bound`. So the win is per EMISSION, not per gesture.
 *   2. DOES `connect_after` WIN? "mark-set" is G_SIGNAL_RUN_LAST, so an after-handler runs
 *      after the class closure that claims. Predicted: yes, deterministically, no polling.
 *   3. ⚠ THE TRAP. GTK clears PRIMARY only when the content is still ITS OWN
 *      (gtktextbuffer.c:3927-3937, guarded on `get_content == priv->selection_content`).
 *      Once the application owns it, GTK will NEVER relinquish on the application's behalf.
 *      Handle only the has-selection case and the application's stale text stays on PRIMARY
 *      after the user deselects — silent, and worse than the defect being fixed.
 *
 * MODES. The trap/relinquish pair is the point; --deferred tests the obvious remedy.
 *   --claims      count GTK claims and after-handler wins for one programmatic selection
 *   --relinquish  select, then DELETE the selection, WITH an immediate has-selection hook
 *   --trap        the same, WITHOUT the hook — expect PRIMARY to keep stale content
 *   --deferred    as --relinquish, but the relinquish re-checks at idle instead of acting
 *                 on the notify
 *
 * ── MEASURED 2026-08-21, GTK 4.22.4 / Quartz. THE DESIGN DOES NOT WORK, AND TWO EARLIER
 *    RESULTS FROM THIS PROBE WERE ARTIFACTS OF THE PROBE ITSELF. ───────────────────────
 *
 * Three-way control, one programmatic `select_range` of offsets 4..19:
 *
 *   --baseline  no handlers at all          2 emissions   selection survives 4..19
 *   --observe   handler, never touches the  2 emissions   selection survives 4..19
 *               clipboard
 *   --trap      handler calls set_content   3 emissions   SELECTION DESTROYED, synchronously
 *
 * THE FINDING: `gdk_clipboard_set_content(primary, ours)` DESTROYS THE BUFFER'S SELECTION.
 * Displacing GtkTextBuffer's provider tells that provider it has lost ownership, and the
 * buffer responds by dropping the selection — the X11 PRIMARY convention, applied on Quartz
 * too. It happens inside the call, and it emits a further mark-set, which is where the
 * "third emission" came from.
 *
 * So the overwrite design defeats itself at the first claim: the selection it is publishing
 * ceases to exist the moment it publishes it. This is not a trigger problem and no
 * relinquish hook can fix it.
 *
 * ⚠ TWO EARLIER RESULTS FROM THIS FILE WERE WRONG, and both were the instrument measuring
 * itself. They are recorded rather than quietly corrected, because both were reported to
 * another seat and one of them caused a correct design to be revised:
 *   - "THREE qualifying emissions, not two." There are TWO. The third was produced by this
 *     probe's own set_content. The original prediction was right.
 *   - "The claim and relinquish halves race each other; a selection change passes through a
 *     transient no-selection state." There is no transient. The selection was destroyed by
 *     the claim, so the relinquish was firing correctly on a genuinely empty selection. The
 *     symptom was real; the explanation was invented to fit it.
 * What was missing was a control in which the handler ran WITHOUT touching the clipboard.
 * `--baseline` and `--observe` are that control, and adding them inverted the conclusion.
 * A probe whose every mode perturbs the subject cannot tell its own effect from the
 * subject's behaviour.
 *
 * THE TRAP result stands, and is unaffected: once content is foreign, GTK never relinquishes
 * PRIMARY on the application's behalf (gtktextbuffer.c:3927-3937, guarded on ownership).
 */

#include <gtk/gtk.h>
#include <stdio.h>
#include <string.h>

typedef enum { MODE_CLAIMS, MODE_RELINQUISH, MODE_TRAP, MODE_DEFERRED, MODE_MIRROR, MODE_BASELINE, MODE_OBSERVE } Mode;
static Mode mode = MODE_CLAIMS;

static GdkContentProvider *ours = NULL;
static GdkClipboard *primary = NULL;

static int qualifying_marks = 0;   /* mark-set emissions for insert/selection_bound */
static int gtk_owned_on_entry = 0; /* of those, how many found GTK's content in place */
static int we_won = 0;             /* of those, how many ended with OURS in place */

static GdkContentProvider *make_ours(void)
{
    GValue v = G_VALUE_INIT;
    g_value_init(&v, G_TYPE_STRING);
    g_value_set_string(&v, "scribobulate-probe-payload");
    GdkContentProvider *p = gdk_content_provider_new_for_value(&v);
    g_value_unset(&v);
    return p;
}

/* The MIRROR decision: GTK does both sides in ONE function with an if/else
 * (update_selection_clipboards, gtktextbuffer.c:3912-3937). Doing the same thing at the
 * same instants means there is no second clock to get out of step with — whatever
 * transients a gesture passes through, GTK's LAST emission fixes the content and this
 * handler on that same emission fixes ownership. */
static void mirror_decide(GtkTextBuffer *buf, const char *why)
{
    GtkTextIter s, e;
    if (gtk_text_buffer_get_selection_bounds(buf, &s, &e)) {
        gdk_clipboard_set_content(primary, ours);
        if (gdk_clipboard_get_content(primary) == ours)
            we_won++;
    } else if (gdk_clipboard_get_content(primary) == ours) {
        printf("    [%s] no selection and we own it -> relinquish\n", why);
        gdk_clipboard_set_content(primary, NULL);
    }
}

static void on_changed(GtkTextBuffer *buf, gpointer data)
{
    (void)data;
    if (mode == MODE_MIRROR)
        mirror_decide(buf, "changed");
}

/* AFTER the class closure, which is where GTK claims. */
static void on_mark_set(GtkTextBuffer *buf, const GtkTextIter *loc, GtkTextMark *mark,
                        gpointer data)
{
    (void)loc;
    (void)data;
    const char *name = gtk_text_mark_get_name(mark);
    if (!name || (strcmp(name, "insert") && strcmp(name, "selection_bound")))
        return;
    qualifying_marks++;

    GtkTextIter s, e;
    gboolean has = gtk_text_buffer_get_selection_bounds(buf, &s, &e);
    GdkContentProvider *before = gdk_clipboard_get_content(primary);
    if (before && before != ours)
        gtk_owned_on_entry++;

    /* INSTRUMENT, not a guess: name every emission and say whether it carried a selection.
     * The count alone cannot tell a setup emission from a gesture emission, and the third
     * emission this probe measured is exactly that ambiguity. */
    printf("    mark-set #%d: %-16s selection=%s owner-on-entry=%s\n", qualifying_marks, name,
           has ? "yes" : "NO ",
           before == NULL ? "none" : (before == ours ? "ours" : "gtk"));

    if (mode == MODE_OBSERVE)
        return; /* observe only: never touch the clipboard */
    if (mode == MODE_MIRROR) {
        mirror_decide(buf, "mark-set");
        return;
    }
    if (has) {
        gdk_clipboard_set_content(primary, ours);
        if (gdk_clipboard_get_content(primary) == ours)
            we_won++;
    }
}

/* The RELINQUISH half. GTK will not do this for us once we own the content. */
static GtkTextBuffer *watched = NULL;

/* Re-check at idle instead of acting on the notify. A selection CHANGE passes through a
 * transient no-selection state, so the immediate form relinquishes in the middle of the
 * very gesture it is meant to follow. */
static gboolean relinquish_if_still_empty(gpointer data)
{
    (void)data;
    GtkTextIter s, e;
    if (watched && !gtk_text_buffer_get_selection_bounds(watched, &s, &e) &&
        gdk_clipboard_get_content(primary) == ours) {
        printf("  relinquishing PRIMARY at idle (still no selection, and we own it)\n");
        gdk_clipboard_set_content(primary, NULL);
    }
    return G_SOURCE_REMOVE;
}

static void on_has_selection(GObject *obj, GParamSpec *pspec, gpointer data)
{
    (void)pspec;
    (void)data;
    gboolean has = FALSE;
    g_object_get(obj, "has-selection", &has, NULL);
    if (has)
        return;
    if (mode == MODE_DEFERRED) {
        g_idle_add(relinquish_if_still_empty, NULL);
        return;
    }
    if (gdk_clipboard_get_content(primary) == ours) {
        printf("  relinquishing PRIMARY (selection gone, and we own it)\n");
        gdk_clipboard_set_content(primary, NULL);
    }
}

static void on_activate(GtkApplication *app, gpointer data)
{
    (void)data;
    GtkWidget *win = gtk_application_window_new(app);
    GtkWidget *view = gtk_text_view_new();
    GtkTextBuffer *buf = gtk_text_buffer_new(NULL);
    gtk_text_buffer_set_text(buf, "the quick brown fox jumps over the lazy dog", -1);
    gtk_text_view_set_buffer(GTK_TEXT_VIEW(view), buf);
    watched = buf;
    gtk_window_set_child(GTK_WINDOW(win), view);
    gtk_window_present(GTK_WINDOW(win));

    for (int i = 0; i < 200; i++)
        g_main_context_iteration(NULL, FALSE);
    printf("  realized=%d\n", gtk_widget_get_realized(view));

    primary = gtk_widget_get_primary_clipboard(view);
    ours = make_ours();

    if (mode != MODE_BASELINE) {
        g_signal_connect_after(buf, "mark-set", G_CALLBACK(on_mark_set), NULL);
        g_signal_connect_after(buf, "changed", G_CALLBACK(on_changed), NULL);
    }
    if (mode != MODE_TRAP && mode != MODE_MIRROR && mode != MODE_BASELINE &&
        mode != MODE_OBSERVE)
        g_signal_connect(buf, "notify::has-selection", G_CALLBACK(on_has_selection), NULL);

    /* ── one user-visible selection change ─────────────────────────────────────── */
    GtkTextIter s, e;
    gtk_text_buffer_get_iter_at_offset(buf, &s, 4);
    gtk_text_buffer_get_iter_at_offset(buf, &e, 19);
    gtk_text_buffer_select_range(buf, &s, &e);
    {
        GtkTextIter a, b;
        gboolean now = gtk_text_buffer_get_selection_bounds(buf, &a, &b);
        printf("  immediately after select_range: selection=%s", now ? "yes" : "NO");
        if (now)
            printf(" (%d..%d)", gtk_text_iter_get_offset(&a), gtk_text_iter_get_offset(&b));
        printf("\n");
    }
    for (int i = 0; i < 50; i++)
        g_main_context_iteration(NULL, FALSE);

    /* DOES THE SELECTION EVEN SURVIVE? Everything below is meaningless if it does not,
     * and a probe that measures ownership across a selection that silently collapsed
     * reports a race that never happened. */
    {
        GtkTextIter cs, ce;
        gboolean still = gtk_text_buffer_get_selection_bounds(buf, &cs, &ce);
        printf("  SELECTION STILL PRESENT AFTER THE GESTURE: %s", still ? "yes" : "NO");
        if (still)
            printf(" (%d..%d)", gtk_text_iter_get_offset(&cs), gtk_text_iter_get_offset(&ce));
        printf("\n");
        if (!still)
            printf("  ⚠ the probe's own selection did not survive; ownership numbers below\n"
                   "    describe an empty selection, not a race\n");
    }
    printf("  qualifying mark-set emissions: %d\n", qualifying_marks);
    printf("  ... of those, GTK's content was in place on entry: %d\n", gtk_owned_on_entry);
    printf("  ... of those, we ended up the owner:               %d\n", we_won);
    printf("  owner after the selection: %s\n",
           gdk_clipboard_get_content(primary) == ours ? "OURS" : "not ours");

    if (mode != MODE_CLAIMS) {
        /* ── the trap: delete the selection and see who relinquishes ───────────── */
        printf("  deleting the selection (%s notify::has-selection hook) ...\n",
               mode == MODE_TRAP ? "WITHOUT" : "with");
        gtk_text_buffer_delete_selection(buf, FALSE, TRUE);
        for (int i = 0; i < 50; i++)
            g_main_context_iteration(NULL, FALSE);

        GdkContentProvider *now = gdk_clipboard_get_content(primary);
        printf("  owner after the delete: %s\n",
               now == NULL ? "NOBODY (relinquished)" : (now == ours ? "STILL OURS" : "GTK"));
        if (mode == MODE_TRAP && now == ours)
            printf("  CONFIRMED: without the hook our stale content stays on PRIMARY\n");
        if (mode == MODE_TRAP && now != ours)
            printf("  UNEXPECTED: something relinquished for us; the trap does not bite here\n");
        if (mode == MODE_RELINQUISH && now == ours)
            printf("  UNEXPECTED: the hook did not fire or did not clear\n");
    }

    fflush(stdout);
    gtk_window_destroy(GTK_WINDOW(win));
}

int main(int argc, char **argv)
{
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--claims"))
            mode = MODE_CLAIMS;
        else if (!strcmp(argv[i], "--relinquish"))
            mode = MODE_RELINQUISH;
        else if (!strcmp(argv[i], "--trap"))
            mode = MODE_TRAP;
        else if (!strcmp(argv[i], "--deferred"))
            mode = MODE_DEFERRED;
        else if (!strcmp(argv[i], "--mirror"))
            mode = MODE_MIRROR;
        else if (!strcmp(argv[i], "--baseline"))
            mode = MODE_BASELINE;
        else if (!strcmp(argv[i], "--observe"))
            mode = MODE_OBSERVE;
        else {
            fprintf(stderr, "unknown argument: %s\n", argv[i]);
            return 2;
        }
    }
    printf("mode: %s\n", mode == MODE_CLAIMS      ? "--claims"
                         : mode == MODE_RELINQUISH ? "--relinquish (immediate hook)"
                         : mode == MODE_DEFERRED    ? "--deferred (idle re-check hook)"
                         : mode == MODE_MIRROR      ? "--mirror (two-sided, mark-set + changed)"
                         : mode == MODE_BASELINE    ? "--baseline (NO handlers at all)"
                         : mode == MODE_OBSERVE     ? "--observe (handler, but never touches the clipboard)"
                                                   : "--trap (no hook)");
    fflush(stdout);

    GtkApplication *app =
        gtk_application_new("org.scribobulate.probe.tpo", G_APPLICATION_NON_UNIQUE);
    g_signal_connect(app, "activate", G_CALLBACK(on_activate), NULL);
    int status = g_application_run(G_APPLICATION(app), 0, NULL);
    g_object_unref(app);
    return status;
}
