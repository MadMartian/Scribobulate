/* Is PRIMARY functionally live on Quartz, or merely present?
   CRITICAL: this NEVER calls gtk_text_buffer_add_selection_clipboard itself.
   GtkTextView is supposed to do that at realize; whether it does on this backend
   is the whole question, and a probe that wires it by hand cannot answer it. */
#include <gtk/gtk.h>
#include <string.h>

static GMainLoop *loop;
static GtkWidget *view;
static GtkTextBuffer *buf;
static const char *SEL = "hello selected world";

static void pump(int ms) {
  gint64 end = g_get_monotonic_time() + ms * 1000;
  while (g_get_monotonic_time() < end)
    g_main_context_iteration(NULL, FALSE);
}

static void dump_clipboard(const char *label, GdkClipboard *cb) {
  GdkContentFormats *f = gdk_clipboard_get_formats(cb);
  char *s = gdk_content_formats_to_string(f);
  GdkContentProvider *p = gdk_clipboard_get_content(cb);
  g_print("  %-10s local=%-5s provider=%-8s\n", label,
          gdk_clipboard_is_local(cb) ? "YES" : "no",
          p ? "present" : "NULL");
  g_print("             formats: %s\n", s && *s ? s : "(empty)");
  gboolean has_buf = gdk_content_formats_contain_gtype(f, GTK_TYPE_TEXT_BUFFER);
  gboolean has_txt = gdk_content_formats_contain_mime_type(f, "text/plain;charset=utf-8");
  g_print("             GTK_TYPE_TEXT_BUFFER=%s  text/plain=%s\n",
          has_buf ? "YES" : "no", has_txt ? "YES" : "no");
  g_free(s);
}

static void on_read(GObject *src, GAsyncResult *res, gpointer tag) {
  GError *e = NULL;
  char *t = gdk_clipboard_read_text_finish(GDK_CLIPBOARD(src), res, &e);
  if (e) { g_print("  %s read_text -> ERROR: %s\n", (char*)tag, e->message); g_error_free(e); }
  else g_print("  %s read_text -> %s\n", (char*)tag, t ? t : "(NULL)");
  g_free(t);
}

static gboolean go(gpointer u) {
  GdkDisplay *d = gdk_display_get_default();
  GdkClipboard *pri = gdk_display_get_primary_clipboard(d);
  GdkClipboard *cli = gdk_display_get_clipboard(d);

  g_print("backend: %s\n", G_OBJECT_TYPE_NAME(d));
  g_print("primary clipboard object: %s\n\n", pri ? G_OBJECT_TYPE_NAME(pri) : "NULL");

  g_print("== BEFORE any selection ==\n");
  dump_clipboard("PRIMARY", pri);

  /* The automatic path: realize a real GtkTextView, then select. */
  g_print("\n  view realized: %s\n", gtk_widget_get_realized(view) ? "YES" : "no");
  GtkTextIter s, e;
  gtk_text_buffer_get_bounds(buf, &s, &e);
  gtk_text_buffer_select_range(buf, &s, &e);
  pump(300);

  g_print("\n== AFTER select-all in a realized GtkTextView (no manual add_selection_clipboard) ==\n");
  dump_clipboard("PRIMARY", pri);
  gdk_clipboard_read_text_async(pri, NULL, on_read, "PRIMARY");
  pump(400);

  /* Control: an explicit copy must populate CLIPBOARD, proving the rig can see
     a populated clipboard at all. If this is empty too, the probe is broken. */
  gtk_text_buffer_copy_clipboard(buf, cli);
  pump(300);
  g_print("\n== CONTROL: explicit copy_clipboard to CLIPBOARD ==\n");
  dump_clipboard("CLIPBOARD", cli);
  gdk_clipboard_read_text_async(cli, NULL, on_read, "CLIPBOARD");
  pump(400);

  /* Now the manual wiring, to isolate whether realize did it. */
  gtk_text_buffer_add_selection_clipboard(buf, pri);
  gtk_text_buffer_get_bounds(buf, &s, &e);
  gtk_text_buffer_select_range(buf, &s, &e);
  gtk_text_buffer_get_iter_at_offset(buf, &s, 0);
  gtk_text_buffer_get_iter_at_offset(buf, &e, 5);
  gtk_text_buffer_select_range(buf, &s, &e);   /* change selection to force publish */
  pump(300);
  g_print("\n== AFTER manual add_selection_clipboard + selection change ==\n");
  dump_clipboard("PRIMARY", pri);
  gdk_clipboard_read_text_async(pri, NULL, on_read, "PRIMARY");
  pump(400);

  g_main_loop_quit(loop);
  return G_SOURCE_REMOVE;
}

int main(void) {
  gtk_init();
  GtkWidget *win = gtk_window_new();
  view = gtk_text_view_new();
  buf = gtk_text_view_get_buffer(GTK_TEXT_VIEW(view));
  gtk_text_buffer_set_text(buf, SEL, -1);
  gtk_window_set_child(GTK_WINDOW(win), view);
  gtk_window_present(GTK_WINDOW(win));
  pump(600);
  loop = g_main_loop_new(NULL, FALSE);
  g_idle_add(go, NULL);
  g_main_loop_run(loop);
  return 0;
}
