/*
 * listview-scroll-snap.c -- does a GtkListView's vadjustment SNAP TO THE BOTTOM
 * during a fast upward wheel scroll, and what does it take to provoke it?
 *
 * Measured in the app first (Scribobulate's outline sidebar, GTK 4.6.9/X11/cairo):
 * scrolling the ~540-row heading list up quickly makes the vadjustment jump to
 * exactly `upper - page_size` — the very bottom — several times during one gesture,
 * with no application code touching the adjustment. This probe isolates which
 * ingredient carries the fault: the GtkTreeListModel, the NON-UNIFORM row heights
 * (the app gives each heading level its own font size), or plain GtkListView.
 *
 * Usage: ./listview-scroll-snap <flat|varied|tree> [rows]
 *   flat   -- GtkStringList, one uniform label per row
 *   varied -- GtkStringList, row height cycles by "heading level" (font size)
 *   tree   -- GtkTreeListModel over a 3-level forest, fully expanded, varied heights
 *
 * It prints every vadjustment value-changed/changed as `V value upper page` /
 * `C value upper page`, so a driver script can look for a value that lands on
 * upper-page_size mid-gesture. No application code writes the adjustment here.
 */

#include <gtk/gtk.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int n_rows = 540;
static const char *mode = "flat";

static void on_value_changed(GtkAdjustment *a, gpointer u) {
  (void)u;
  printf("V %.1f %.1f %.1f\n", gtk_adjustment_get_value(a),
         gtk_adjustment_get_upper(a), gtk_adjustment_get_page_size(a));
  fflush(stdout);
}

static void on_changed(GtkAdjustment *a, gpointer u) {
  (void)u;
  printf("C %.1f %.1f %.1f\n", gtk_adjustment_get_value(a),
         gtk_adjustment_get_upper(a), gtk_adjustment_get_page_size(a));
  fflush(stdout);
}

/* Row height varies by level, the way the app's per-level CSS classes make it. */
static const char *level_css(int level) {
  static const char *c[] = {"lvl0", "lvl1", "lvl2", "lvl3", "lvl4"};
  return c[level % 5];
}

static void setup_cb(GtkSignalListItemFactory *f, GtkListItem *item, gpointer u) {
  (void)f; (void)u;
  GtkWidget *label = gtk_label_new("");
  gtk_label_set_xalign(GTK_LABEL(label), 0.0);
  gtk_label_set_ellipsize(GTK_LABEL(label), PANGO_ELLIPSIZE_END);
  if (strcmp(mode, "tree") == 0) {
    GtkWidget *expander = gtk_tree_expander_new();
    gtk_tree_expander_set_child(GTK_TREE_EXPANDER(expander), label);
    gtk_list_item_set_child(item, expander);
  } else {
    gtk_list_item_set_child(item, label);
  }
}

static void bind_cb(GtkSignalListItemFactory *f, GtkListItem *item, gpointer u) {
  (void)f; (void)u;
  GtkWidget *child = gtk_list_item_get_child(item);
  GtkWidget *label = child;
  const char *text = "row";
  int level = 0;

  if (strcmp(mode, "tree") == 0) {
    GtkTreeListRow *row = GTK_TREE_LIST_ROW(gtk_list_item_get_item(item));
    gtk_tree_expander_set_list_row(GTK_TREE_EXPANDER(child), row);
    label = gtk_tree_expander_get_child(GTK_TREE_EXPANDER(child));
    GtkStringObject *so = GTK_STRING_OBJECT(gtk_tree_list_row_get_item(row));
    text = gtk_string_object_get_string(so);
    level = (int)gtk_tree_list_row_get_depth(row);
    g_object_unref(so);
  } else {
    GtkStringObject *so = GTK_STRING_OBJECT(gtk_list_item_get_item(item));
    text = gtk_string_object_get_string(so);
    level = strcmp(mode, "varied") == 0 ? (int)(gtk_list_item_get_position(item) % 5) : 0;
  }
  gtk_label_set_text(GTK_LABEL(label), text);
  const char *cls[] = {level_css(level), NULL};
  gtk_widget_set_css_classes(label, (const char **)cls);
}

/* Children of a "tree" row: two leaves under every top-level entry, one level deep. */
static GListModel *tree_children(gpointer item, gpointer u) {
  (void)u;
  GtkStringObject *so = GTK_STRING_OBJECT(item);
  const char *s = gtk_string_object_get_string(so);
  if (s[0] != 'H') /* only top-level "H<n>" rows have children */
    return NULL;
  GtkStringList *kids = gtk_string_list_new(NULL);
  for (int i = 0; i < 2; i++) {
    char buf[64];
    g_snprintf(buf, sizeof buf, "  child %s.%d", s + 1, i);
    gtk_string_list_append(kids, buf);
  }
  return G_LIST_MODEL(kids);
}

static void expand_all(GtkTreeListModel *m) {
  guint i = 0;
  while (i < g_list_model_get_n_items(G_LIST_MODEL(m))) {
    GtkTreeListRow *row = GTK_TREE_LIST_ROW(g_list_model_get_item(G_LIST_MODEL(m), i));
    if (row) {
      gtk_tree_list_row_set_expanded(row, TRUE);
      g_object_unref(row);
    }
    i++;
  }
}

/* One programmatic upward step; starts by jumping to the bottom. */
static gboolean auto_scroll_tick(gpointer user_data) {
  GtkAdjustment *a = GTK_ADJUSTMENT(user_data);
  double *stepp = g_object_get_data(G_OBJECT(a), "step");
  double max = gtk_adjustment_get_upper(a) - gtk_adjustment_get_page_size(a);
  static gboolean primed = FALSE;
  if (max < 100) return G_SOURCE_CONTINUE; /* not laid out yet */
  if (!primed) {
    const char *start = g_getenv("PROBE_START");
    gtk_adjustment_set_value(a, start ? g_ascii_strtod(start, NULL) : max);
    primed = TRUE;
    return G_SOURCE_CONTINUE;
  }
  double v = gtk_adjustment_get_value(a) - *stepp;
  if (v <= 0) return G_SOURCE_REMOVE;
  gtk_adjustment_set_value(a, v);
  return G_SOURCE_CONTINUE;
}

/* PROBE_COUNT=1: report how many row widgets the ListView actually holds, plus its
 * OWN scrollable adjustment — the two facts that say whether virtualization survived
 * a wrapper. */
static gboolean report_rows(gpointer list) {
  int n = 0;
  for (GtkWidget *c = gtk_widget_get_first_child(GTK_WIDGET(list)); c;
       c = gtk_widget_get_next_sibling(c))
    n++;
  GtkAdjustment *own = gtk_scrollable_get_vadjustment(GTK_SCROLLABLE(list));
  printf("ROWS %d own_value=%.1f own_upper=%.1f own_page=%.1f height=%d\n", n,
         own ? gtk_adjustment_get_value(own) : -1.0,
         own ? gtk_adjustment_get_upper(own) : -1.0,
         own ? gtk_adjustment_get_page_size(own) : -1.0,
         gtk_widget_get_height(GTK_WIDGET(list)));
  fflush(stdout);
  return G_SOURCE_CONTINUE;
}

static void activate(GtkApplication *app, gpointer u) {
  (void)u;
  GtkCssProvider *css = gtk_css_provider_new();
  gtk_css_provider_load_from_data(css,
      ".lvl0{font-size:16pt;font-weight:bold;}"
      ".lvl1{font-size:13pt;}"
      ".lvl2{font-size:11pt;}"
      ".lvl3{font-size:10pt;}"
      ".lvl4{font-size:9pt;}", -1);
  gtk_style_context_add_provider_for_display(gdk_display_get_default(),
      GTK_STYLE_PROVIDER(css), GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);

  GtkStringList *roots = gtk_string_list_new(NULL);
  for (int i = 0; i < n_rows; i++) {
    char buf[64];
    g_snprintf(buf, sizeof buf, "%c%d some heading text here", strcmp(mode, "tree") == 0 ? 'H' : 'R', i);
    gtk_string_list_append(roots, buf);
  }

  GtkSelectionModel *sel;
  if (strcmp(mode, "tree") == 0) {
    GtkTreeListModel *tree = gtk_tree_list_model_new(G_LIST_MODEL(roots), FALSE, FALSE, tree_children, NULL, NULL);
    expand_all(tree);
    sel = GTK_SELECTION_MODEL(gtk_single_selection_new(G_LIST_MODEL(tree)));
  } else {
    sel = GTK_SELECTION_MODEL(gtk_single_selection_new(G_LIST_MODEL(roots)));
  }
  gtk_single_selection_set_autoselect(GTK_SINGLE_SELECTION(sel), FALSE);
  gtk_single_selection_set_can_unselect(GTK_SINGLE_SELECTION(sel), TRUE);

  /* PROBE_SELECT=<pos>: pre-select (and focus) one row, to tell an anchor fault
   * apart from a "scroll the focused/selected row back into view" fault. */
  if (g_getenv("PROBE_SELECT"))
    gtk_single_selection_set_selected(GTK_SINGLE_SELECTION(sel),
                                      (guint)atoi(g_getenv("PROBE_SELECT")));

  GtkListItemFactory *factory = gtk_signal_list_item_factory_new();
  g_signal_connect(factory, "setup", G_CALLBACK(setup_cb), NULL);
  g_signal_connect(factory, "bind", G_CALLBACK(bind_cb), NULL);

  GtkWidget *list = gtk_list_view_new(sel, factory);
  GtkWidget *sw = gtk_scrolled_window_new();
  if (g_getenv("PROBE_WRAP_BOX")) {
    /* A GtkBox is not GtkScrollable, so the ScrolledWindow interposes a GtkViewport
     * and the ListView is allocated its FULL height — virtualization (and with it
     * the anchor machinery) is out of the picture. */
    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_box_append(GTK_BOX(box), list);
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(sw), box);
  } else {
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(sw), list);
  }
  gtk_widget_set_vexpand(sw, TRUE);

  /* Workaround experiments, switched by env var so one binary tests them all. */
  if (g_getenv("PROBE_NO_KINETIC"))
    gtk_scrolled_window_set_kinetic_scrolling(GTK_SCROLLED_WINDOW(sw), FALSE);

  GtkAdjustment *vadj = gtk_scrolled_window_get_vadjustment(GTK_SCROLLED_WINDOW(sw));
  g_signal_connect(vadj, "value-changed", G_CALLBACK(on_value_changed), NULL);
  g_signal_connect(vadj, "changed", G_CALLBACK(on_changed), NULL);

  /* PROBE_AUTO=<px>: drive the adjustment DIRECTLY (no scroll controller, no
   * animation) downward-to-upward at one step per 8 ms, to tell a GtkScrolledWindow
   * input-path fault apart from a GtkListBase anchor fault. */
  if (g_getenv("PROBE_AUTO")) {
    double step = g_ascii_strtod(g_getenv("PROBE_AUTO"), NULL);
    double *stepp = g_new(double, 1);
    *stepp = step;
    g_object_set_data(G_OBJECT(vadj), "step", stepp);
    g_timeout_add(8, (GSourceFunc)auto_scroll_tick, vadj);
    (void)0;
  }

  if (g_getenv("PROBE_COUNT"))
    g_timeout_add(2000, report_rows, list);

  GtkWidget *win = gtk_application_window_new(app);
  gtk_window_set_title(GTK_WINDOW(win), "listview-scroll-snap");
  gtk_window_set_default_size(GTK_WINDOW(win), 300, 800);
  gtk_window_set_child(GTK_WINDOW(win), sw);
  gtk_window_present(GTK_WINDOW(win));
}

int main(int argc, char **argv) {
  if (argc > 1) mode = argv[1];
  if (argc > 2) n_rows = atoi(argv[2]);
  GtkApplication *app = gtk_application_new("org.scribobulate.probe.listviewscrollsnap",
                                            G_APPLICATION_NON_UNIQUE);
  g_signal_connect(app, "activate", G_CALLBACK(activate), NULL);
  int status = g_application_run(G_APPLICATION(app), 1, argv);
  g_object_unref(app);
  return status;
}
