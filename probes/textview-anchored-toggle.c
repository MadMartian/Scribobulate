/*
 * textview-anchored-toggle.c — does a SMALL anchored child cost what a LARGE one costs?
 *
 * SUBJECT
 * -------
 * PLAN.accessibility.md states that making a self-drawn preview element into a real
 * widget at a GtkTextChildAnchor is "architecturally forbidden here", justifying it by
 * height-for-width re-measurement and the layout churn that blanks the view (ScrAP-23).
 *
 * That justification is an EMPIRICAL claim, and the mechanism it names —
 * gtk_text_view_measure doing `min = MAX(min, child_min)` over every anchored child —
 * is PROPORTIONAL TO THE CHILD. A 900px table drives the view's minimum width. The
 * open question is whether a ~16px disclosure toggle does anything measurable at all.
 *
 * This probe answers that with numbers instead of argument. It does not argue about
 * height-for-width; it measures minimum width directly, in four configurations, and
 * then checks whether the ScrAP-23a horizontal-overflow chain is reachable through a
 * small child.
 *
 * WHY C, AND WHY IT MATTERS HERE
 * ------------------------------
 * Per probes/README.md: a C probe runs against an ARBITRARY installed GTK. The
 * measurement below is expected to be vintage-dependent (the anchored-child parking
 * coordinate changed in 4.20.1, commit a4b4fec72e), so the mac and Windows seats must
 * be able to re-run this against 4.22.x without this crate building. Nothing about the
 * subject touches the Rust binding, so a Rust probe would only narrow who can run it.
 *
 * WHAT IT DELIBERATELY DOES NOT MEASURE
 * -------------------------------------
 * Accessible STATE round-trip. GTK4 has no public getter for GTK_ACCESSIBLE_STATE_EXPANDED
 * — it is only observable from the far side of the AT-SPI bus, so it belongs in a
 * pyatspi client rig, not here. This probe reports the accessible ROLE (which does have
 * a public getter) and stops there rather than pretending to prove the other half.
 *
 * BUILD
 *   cc probes/textview-anchored-toggle.c -o /tmp/textview-anchored-toggle \
 *      $(pkg-config --cflags --libs gtk4)
 *
 * RUN
 *   /tmp/textview-anchored-toggle minwidth   # headless-safe (Xvfb fine)
 *   /tmp/textview-anchored-toggle overflow   # headless-safe (Xvfb fine)
 *   /tmp/textview-anchored-toggle role       # headless-safe (Xvfb fine)
 *   /tmp/textview-anchored-toggle keys       # NEEDS A REAL DISPLAY + A HUMAN
 *
 * The `keys` mode is interactive on purpose; see its own comment for why it cannot be
 * automated into a meaningful result.
 */

#include <gtk/gtk.h>

/* The disclosure affordance under test. Composing GtkToggleButton rather than
 * hand-rolling a widget is deliberate: GtkToggleButton already calls
 * gtk_widget_set_receives_default(TRUE) (gtkbutton.c:435) AND installs the five
 * activation keyvals (gtkbutton.c:316-325). A hand-rolled widget that only calls
 * gtk_widget_class_set_activate_signal() gets Space unconditionally but Enter ONLY
 * while the window has no default widget — which is why `keys` below installs one. */
#define TOGGLE_PX 16

/* A stand-in for a Markdown table: a wide, fixed-minimum child. This is the child the
 * rule was actually written about. */
#define WIDE_PX 900

/* The disclosure indicator is the ENTIRE feedback channel. Measured 2026-08-31 on the
 * operator's live session: a build that fired `toggled` 29 times without swapping the
 * icon was reported as "did not do anything when clicking on it". The signal is not the
 * affordance — the icon is. Collapsed = pan-end-symbolic, expanded = pan-down-symbolic
 * (Adwaita _common.scss:3441-3451).
 *
 * NOTE for the real implementation: the RTL variant is a separate icon NAME, not a
 * mirror of this one, so self-drawing the triangle means owning RTL yourself. */
static GtkWidget *make_toggle(void) {
    GtkWidget *b = gtk_toggle_button_new();
    GtkWidget *img = gtk_image_new_from_icon_name("pan-end-symbolic");
    gtk_image_set_pixel_size(GTK_IMAGE(img), TOGGLE_PX);
    gtk_button_set_child(GTK_BUTTON(b), img);
    gtk_widget_set_halign(b, GTK_ALIGN_START);
    /* VERTICAL ALIGNMENT IS NOT AUTOMATIC, and getting it wrong is visible immediately.
     * GTK_ALIGN_START pins the child to the top of the line box, so the indicator rides
     * high against its own summary text — reported from the live session 2026-08-31,
     * where the arrow sat noticeably above the "Details" baseline. A disclosure
     * indicator has to sit on the text baseline like a glyph, not float above it.
     * BASELINE is the alignment that expresses that intent; whether GtkTextView honours
     * it for an anchored child (rather than falling back to FILL) is a live question,
     * so this is CHANGED-AND-UNVERIFIED until someone looks at it again. */
    gtk_widget_set_valign(b, GTK_ALIGN_BASELINE);
    gtk_accessible_update_state(GTK_ACCESSIBLE(b),
                                GTK_ACCESSIBLE_STATE_EXPANDED, FALSE, -1);
    /* An anchored child sits INSIDE the text area, which sets an I-beam cursor, and it
     * does NOT participate in the view's own hover machinery (which drives the cursor
     * from a motion handler on the view — src/preview/interactions.rs:243). Reported on
     * the live session 2026-08-31: the toggle hovered as an I-bar and read as
     * unclickable. The child must carry its own cursor. "pointer" matches what this
     * project already shows for links, comment markers, task checkboxes and copy
     * buttons, so this is the existing convention rather than a third one. */
    gtk_widget_set_cursor_from_name(b, "pointer");
    return b;
}

static GtkWidget *make_wide(void) {
    GtkWidget *w = gtk_drawing_area_new();
    gtk_widget_set_size_request(w, WIDE_PX, 40);
    return w;
}

/* Fill a buffer with enough prose that the view has real content, and anchor
 * `n_toggles` small children and `n_wide` wide ones at paragraph boundaries. */
static GtkWidget *build_view(int n_toggles, int n_wide) {
    GtkWidget *view = gtk_text_view_new();
    gtk_text_view_set_wrap_mode(GTK_TEXT_VIEW(view), GTK_WRAP_WORD_CHAR);
    GtkTextBuffer *buf = gtk_text_view_get_buffer(GTK_TEXT_VIEW(view));

    for (int i = 0; i < 8; i++) {
        gtk_text_buffer_insert_at_cursor(
            buf, "Ordinary paragraph text that wraps in the preview pane.\n", -1);
    }

    GtkTextIter it;
    for (int i = 0; i < n_toggles; i++) {
        gtk_text_buffer_get_end_iter(buf, &it);
        GtkTextChildAnchor *a = gtk_text_buffer_create_child_anchor(buf, &it);
        gtk_text_view_add_child_at_anchor(GTK_TEXT_VIEW(view), make_toggle(), a);
        gtk_text_buffer_get_end_iter(buf, &it);
        gtk_text_buffer_insert(buf, &it, " summary line\n", -1);
    }
    for (int i = 0; i < n_wide; i++) {
        gtk_text_buffer_get_end_iter(buf, &it);
        GtkTextChildAnchor *a = gtk_text_buffer_create_child_anchor(buf, &it);
        gtk_text_view_add_child_at_anchor(GTK_TEXT_VIEW(view), make_wide(), a);
        gtk_text_buffer_get_end_iter(buf, &it);
        gtk_text_buffer_insert(buf, &it, "\n", -1);
    }
    return view;
}

static int measure_min_width(GtkWidget *view) {
    int min = -1, nat = -1;
    gtk_widget_measure(view, GTK_ORIENTATION_HORIZONTAL, -1, &min, &nat, NULL, NULL);
    return min;
}

/* ── mode: minwidth ────────────────────────────────────────────────────────────
 * The headline number. If a 16px child moved the view's minimum width the way a
 * 900px child does, the rule as written would be correct and the disclosure toggle
 * would be genuinely forbidden. */
static void mode_minwidth(void) {
    struct { const char *label; int toggles; int wide; } cfg[] = {
        { "text only               ", 0,  0 },
        { "text + 1 toggle (16px)  ", 1,  0 },
        { "text + 8 toggles (16px) ", 8,  0 },
        { "text + 1 wide (900px)   ", 0,  1 },
        { "text + 8 toggles + wide ", 8,  1 },
    };
    g_print("config                     min_width\n");
    g_print("-------------------------  ---------\n");
    int baseline = -1;
    for (guint i = 0; i < G_N_ELEMENTS(cfg); i++) {
        GtkWidget *v = build_view(cfg[i].toggles, cfg[i].wide);
        /* Hold a ref so the view survives until measured; it is not in a window. */
        g_object_ref_sink(v);
        int min = measure_min_width(v);
        if (i == 0) baseline = min;
        g_print("%s  %6d%s\n", cfg[i].label, min,
                (i > 0 && min == baseline) ? "   (== baseline)" : "");
        g_object_unref(v);
    }
    g_print("\nRead: if the toggle rows equal the baseline and only the wide row rises,\n"
            "the min-width hazard is proportional to the child and a 16px toggle is free.\n");
}

/* ── mode: overflow ────────────────────────────────────────────────────────────
 * ScrAP-23a's chain: an anchored child that overflows the content column by even a
 * few px arms the Automatic h-scrollbar, whose appear/disappear re-enters the
 * ScrAP-22/23 validation churn that blanks the view. This asks whether a small child
 * can reach that chain at all.
 *
 * Uses a real toplevel and real frame ticks — NOT a tight g_main_context_iteration
 * pump. A pump loop returns allocation state that has not been through a genuine
 * size_allocate, and reading it that way has already produced one wrong measurement
 * on this feature. */
static gboolean overflow_done(gpointer data) {
    GtkScrolledWindow *sw = GTK_SCROLLED_WINDOW(data);
    GtkAdjustment *h = gtk_scrolled_window_get_hadjustment(sw);
    double upper = gtk_adjustment_get_upper(h);
    double page  = gtk_adjustment_get_page_size(h);
    g_print("hadjustment.upper = %.1f\n", upper);
    g_print("hadjustment.page  = %.1f\n", page);
    g_print("overflow          = %.1f px\n", upper - page);
    g_print("\n%s\n", (upper - page) > 0.5
        ? "REACHABLE: a small anchored child overflowed the content column."
        : "NOT REACHABLE: no horizontal overflow from small anchored children.");
    gtk_window_destroy(GTK_WINDOW(gtk_widget_get_root(GTK_WIDGET(sw))));
    return G_SOURCE_REMOVE;
}

static void mode_overflow(void) {
    GtkWidget *win = gtk_window_new();
    gtk_window_set_default_size(GTK_WINDOW(win), 600, 400);
    GtkWidget *sw = gtk_scrolled_window_new();
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(sw),
                                   GTK_POLICY_AUTOMATIC, GTK_POLICY_AUTOMATIC);
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(sw), build_view(8, 0));
    gtk_window_set_child(GTK_WINDOW(win), sw);
    gtk_window_present(GTK_WINDOW(win));
    /* Let real frames run before reading geometry. */
    g_timeout_add(600, overflow_done, sw);
}

/* ── mode: role ────────────────────────────────────────────────────────────────
 * Only half of the accessibility claim is checkable from inside the process. */
static void mode_role(void) {
    GtkWidget *b = make_toggle();
    g_object_ref_sink(b);
    GtkAccessibleRole role = gtk_accessible_get_accessible_role(GTK_ACCESSIBLE(b));
    g_print("accessible role = %d  (GTK_ACCESSIBLE_ROLE_BUTTON = %d)\n",
            (int)role, (int)GTK_ACCESSIBLE_ROLE_BUTTON);
    g_print("%s\n", role == GTK_ACCESSIBLE_ROLE_BUTTON
        ? "MATCH: the anchored affordance carries a real button role."
        : "MISMATCH: role is not BUTTON — check what the toggle composes.");
    g_print("\nNOTE: GTK_ACCESSIBLE_STATE_EXPANDED has no public getter. Its round-trip\n"
            "is observable only across the AT-SPI bus and is measured by the pyatspi rig,\n"
            "not here. This probe deliberately does not claim it.\n");
    g_object_unref(b);
}

/* ── mode: keys ────────────────────────────────────────────────────────────────
 * WHY THIS IS INTERACTIVE RATHER THAN DRIVEN BY XTEST.
 *
 * The hazard is not "Enter does not work". It is that Enter works in a bare test
 * window and SILENTLY STOPS in a window that has a default widget, because
 * gtk_window_real_activate_default (gtkwindow.c:2455-2464) falls back to the FOCUS
 * widget when no default exists. A rig that builds a bare window therefore asserts
 * the fallback and passes while the real dialog would fail.
 *
 * So this window installs a REAL DEFAULT BUTTON before asking anything about Enter.
 * That is the configuration the assertion is only meaningful in.
 *
 * It is left to a human because the thing being judged is whether the affordance
 * responds the way a disclosure control should FEEL — focus ring visible, Space and
 * Enter both firing, Tab reaching it in a sensible order. Those are judgements, and a
 * synthetic keystroke that reports "signal emitted" does not settle them.
 */
/* A probe that cannot say WHICH input caused an activation cannot answer the question
 * it exists for. The first run of this rig printed only the resulting state, so 29
 * activations were consistent with Space working, Enter working, or only the mouse
 * working — and could not distinguish them. Every activation now names its cause. */
static void note_input(GtkWidget *toggle, const char *what) {
    g_object_set_data_full(G_OBJECT(toggle), "last-input", g_strdup(what), g_free);
}

static gboolean on_key(GtkEventControllerKey *c, guint keyval,
                       guint keycode, GdkModifierType state, gpointer user_data) {
    (void)c; (void)keycode; (void)state;
    note_input(GTK_WIDGET(user_data), gdk_keyval_name(keyval));
    return GDK_EVENT_PROPAGATE; /* observe only — never consume */
}

static void on_pressed(GtkGestureClick *g, int n, double x, double y, gpointer user_data) {
    (void)g; (void)n; (void)x; (void)y;
    note_input(GTK_WIDGET(user_data), "mouse-click");
}

static void on_toggled(GtkToggleButton *b, gpointer user_data) {
    (void)user_data;
    gboolean on = gtk_toggle_button_get_active(b);
    gtk_accessible_update_state(GTK_ACCESSIBLE(b),
                                GTK_ACCESSIBLE_STATE_EXPANDED, on, -1);
    /* The indicator swap — the thing whose absence read as "it does nothing". */
    GtkWidget *img = gtk_button_get_child(GTK_BUTTON(b));
    if (GTK_IS_IMAGE(img)) {
        gtk_image_set_from_icon_name(GTK_IMAGE(img),
            on ? "pan-down-symbolic" : "pan-end-symbolic");
    }
    const char *via = g_object_get_data(G_OBJECT(b), "last-input");
    g_print("TOGGLED via %-12s -> %s\n", via ? via : "(unknown)",
            on ? "expanded" : "collapsed");
}

static void mode_keys(void) {
    GtkWidget *win = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(win), "Disclosure toggle — keyboard check");
    gtk_window_set_default_size(GTK_WINDOW(win), 520, 320);

    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 12);
    gtk_widget_set_margin_top(box, 12);
    gtk_widget_set_margin_bottom(box, 12);
    gtk_widget_set_margin_start(box, 12);
    gtk_widget_set_margin_end(box, 12);

    GtkWidget *label = gtk_label_new(
        "Click the ▸ toggle — it must flip to ▾ and back.\n"
        "Then Tab to it and press SPACE, then ENTER. Both must flip it.\n"
        "Every activation now prints WHICH input caused it.\n\n"
        "This window HAS a default button (\"Default\"), which is the condition\n"
        "under which Enter silently stops working on a hand-rolled widget.");
    gtk_label_set_xalign(GTK_LABEL(label), 0.0);
    gtk_box_append(GTK_BOX(box), label);

    GtkWidget *view = gtk_text_view_new();
    gtk_text_view_set_wrap_mode(GTK_TEXT_VIEW(view), GTK_WRAP_WORD_CHAR);
    /* MATCH THE REAL PREVIEW. src/preview/render.rs:75 does set_editable(false); the
     * first version of this probe used GTK's default (editable), where Tab inserts a
     * tab character and can never reach an anchored child. That produced a "cannot Tab
     * to it" result that was a property of the RIG, not of GtkTextView — the exact
     * class of error this probe exists to avoid. */
    gtk_text_view_set_editable(GTK_TEXT_VIEW(view), FALSE);
    gtk_text_view_set_cursor_visible(GTK_TEXT_VIEW(view), FALSE);
    GtkTextBuffer *buf = gtk_text_view_get_buffer(GTK_TEXT_VIEW(view));
    gtk_text_buffer_insert_at_cursor(buf, "Some prose before the disclosure.\n", -1);

    GtkTextIter it;
    gtk_text_buffer_get_end_iter(buf, &it);
    GtkTextChildAnchor *a = gtk_text_buffer_create_child_anchor(buf, &it);
    GtkWidget *toggle = make_toggle();
    g_signal_connect(toggle, "toggled", G_CALLBACK(on_toggled), NULL);
    /* Observers only — both propagate, so neither changes what is being measured. */
    GtkEventController *kc = gtk_event_controller_key_new();
    g_signal_connect(kc, "key-pressed", G_CALLBACK(on_key), toggle);
    gtk_widget_add_controller(toggle, kc);
    GtkGesture *gc = gtk_gesture_click_new();
    gtk_event_controller_set_propagation_phase(GTK_EVENT_CONTROLLER(gc), GTK_PHASE_CAPTURE);
    g_signal_connect(gc, "pressed", G_CALLBACK(on_pressed), toggle);
    gtk_widget_add_controller(toggle, GTK_EVENT_CONTROLLER(gc));
    gtk_text_view_add_child_at_anchor(GTK_TEXT_VIEW(view), toggle, a);
    gtk_text_buffer_get_end_iter(buf, &it);
    gtk_text_buffer_insert(buf, &it, " Details\nSome prose after.\n", -1);

    GtkWidget *frame = gtk_frame_new(NULL);
    gtk_frame_set_child(GTK_FRAME(frame), view);
    gtk_widget_set_vexpand(frame, TRUE);
    gtk_box_append(GTK_BOX(box), frame);

    /* The whole point: a real default widget on the window. */
    GtkWidget *def = gtk_button_new_with_label("Default");
    gtk_widget_set_halign(def, GTK_ALIGN_END);
    gtk_box_append(GTK_BOX(box), def);

    gtk_window_set_child(GTK_WINDOW(win), box);
    gtk_window_present(GTK_WINDOW(win));
    gtk_window_set_default_widget(GTK_WINDOW(win), def);

    g_print("Interactive. Tab to the toggle; press Space, then Enter.\n");
    g_print("Expect a TOGGLED line per activation. Close the window when done.\n\n");
}

/* Quit once every toplevel has been closed, so the interactive modes end when the
 * human closes the window and the headless ones end when their own timeout does. */
static gboolean last_toplevel_gone(gpointer data) {
    GListModel *tops = gtk_window_get_toplevels();
    if (tops && g_list_model_get_n_items(tops) == 0) {
        g_main_loop_quit((GMainLoop *)data);
        return G_SOURCE_REMOVE;
    }
    return G_SOURCE_CONTINUE;
}

int main(int argc, char **argv) {
    gtk_init();
    const char *mode = argc > 1 ? argv[1] : "minwidth";

    if (g_str_equal(mode, "minwidth")) {
        mode_minwidth();
        return 0;
    }
    if (g_str_equal(mode, "role")) {
        mode_role();
        return 0;
    }

    GMainLoop *loop = g_main_loop_new(NULL, FALSE);
    if (g_str_equal(mode, "overflow")) {
        mode_overflow();
    } else if (g_str_equal(mode, "keys")) {
        mode_keys();
    } else {
        g_printerr("unknown mode: %s (minwidth|overflow|role|keys)\n", mode);
        return 2;
    }
    g_timeout_add(200, last_toplevel_gone, loop);
    g_main_loop_run(loop);
    return 0;
}
