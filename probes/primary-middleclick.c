/* Is there a middle-click PRIMARY-paste route on Quartz?
   Selects "AAAA", then waits. An external middle-click over the view should make
   GtkTextView call gtk_text_buffer_paste_clipboard(PRIMARY) and insert "AAAA". */
#include <gtk/gtk.h>
#include <string.h>
static void on_press(GtkGestureClick *g, int n, double x, double y, gpointer u) {
  guint b = gtk_gesture_single_get_current_button(GTK_GESTURE_SINGLE(g));
  g_print("BUTTON PRESS DELIVERED: button=%u at (%.0f,%.0f)\n", b, x, y);
}
static GtkTextBuffer *buf;
static const char *START = "AAAA BBBB";
static gboolean report(gpointer u) {
  GtkTextIter s,e; gtk_text_buffer_get_bounds(buf,&s,&e);
  char *t = gtk_text_buffer_get_text(buf,&s,&e,TRUE);
  g_print("FINAL buffer: %s\n", t);
  g_print("VERDICT: %s\n", strcmp(t, START)==0
      ? "unchanged -- no middle-click PRIMARY paste occurred"
      : "*** CHANGED -- a middle-click PRIMARY paste DID occur ***");
  g_free(t);
  gtk_window_destroy(GTK_WINDOW(u));
  return G_SOURCE_REMOVE;
}
static gboolean arm(gpointer u) {
  GtkTextIter s,e;
  gtk_text_buffer_get_iter_at_offset(buf,&s,0);
  gtk_text_buffer_get_iter_at_offset(buf,&e,4);
  gtk_text_buffer_select_range(buf,&s,&e);      /* publishes "AAAA" to PRIMARY */
  GdkClipboard *pri = gdk_display_get_primary_clipboard(gdk_display_get_default());
  g_print("PRIMARY armed with GtkTextBuffer: %s\n",
    gdk_content_formats_contain_gtype(gdk_clipboard_get_formats(pri), GTK_TYPE_TEXT_BUFFER)?"YES":"no");
  g_print("READY -- middle-click now\n");
  return G_SOURCE_REMOVE;
}
int main(void) {
  gtk_init();
  GtkWidget *win = gtk_window_new();
  GtkWidget *v = gtk_text_view_new();
  buf = gtk_text_view_get_buffer(GTK_TEXT_VIEW(v));
  gtk_text_buffer_set_text(buf, START, -1);
  /* Positive control: prove a click reaches the widget at all. Button 0 = any. */
  GtkGesture *g = gtk_gesture_click_new();
  gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(g), 0);
  gtk_event_controller_set_propagation_phase(GTK_EVENT_CONTROLLER(g), GTK_PHASE_CAPTURE);
  g_signal_connect(g, "pressed", G_CALLBACK(on_press), NULL);
  gtk_widget_add_controller(v, GTK_EVENT_CONTROLLER(g));
  gtk_window_set_child(GTK_WINDOW(win), v);
  gtk_window_set_default_size(GTK_WINDOW(win), 500, 300);
  gtk_window_present(GTK_WINDOW(win));
  g_timeout_add(700, arm, NULL);
  g_timeout_add(20000, report, win);
  while (g_list_model_get_n_items(gtk_window_get_toplevels()) > 0)
    g_main_context_iteration(NULL, TRUE);
  return 0;
}
