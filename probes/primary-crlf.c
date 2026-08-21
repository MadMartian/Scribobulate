/* Q2: with NO hook present (master 7a6be98's state), is a same-app PRIMARY
   paste of a lone-CRLF document byte-clean on Quartz?
   Uses the AUTOMATIC publish path: a realized GtkTextView, select-all, no
   manual add_selection_clipboard. */
#include <gtk/gtk.h>
#include <gtksourceview/gtksource.h>
#include <string.h>

static GMainLoop *loop;
static int emissions = 0;
static const char *DOC = "```rust\r\nfn main() {}\r\n";   /* unclosed fence, CRLF */

static void pump(int ms) {
  gint64 end = g_get_monotonic_time() + ms * 1000;
  while (g_get_monotonic_time() < end) g_main_context_iteration(NULL, FALSE);
}
static void hexp(const char *l, const char *s) {
  g_print("  %-9s ", l);
  for (const unsigned char *p=(const unsigned char*)s; *p; p++) g_print("%02x ", *p);
  g_print("\n");
}
static void on_insert(GtkTextBuffer *b, GtkTextIter *i, char *t, int n, gpointer u) {
  emissions++;
  g_print("    run %d len=%-3d ", emissions, n);
  for (int k=0;k<n;k++) g_print("%02x ", (unsigned char)t[k]);
  g_print("\n");
}

static GtkWidget *view;
static GtkSourceBuffer *src;

static gboolean go(gpointer u) {
  GdkDisplay *d = gdk_display_get_default();
  GdkClipboard *pri = gdk_display_get_primary_clipboard(d);

  GtkTextIter s, e;
  gtk_text_buffer_get_bounds(GTK_TEXT_BUFFER(src), &s, &e);
  gtk_source_buffer_ensure_highlight(src, &s, &e);

  /* toggle audit */
  gtk_text_buffer_get_start_iter(GTK_TEXT_BUFFER(src), &s);
  char *txt = gtk_text_buffer_get_text(GTK_TEXT_BUFFER(src), &s, &e, TRUE);
  int dangerous = 0;
  GtkTextIter it; gtk_text_buffer_get_start_iter(GTK_TEXT_BUFFER(src), &it);
  while (gtk_text_iter_forward_to_tag_toggle(&it, NULL)) {
    int off = gtk_text_iter_get_offset(&it);
    if (txt[off]=='\n' && off>0 && txt[off-1]=='\r') { dangerous++;
      g_print("  toggle at off=%d sits INSIDE the final CRLF\n", off); }
  }
  if (!dangerous) g_print("  no toggle splits a CRLF (fixture not exercising the case!)\n");
  g_free(txt);

  /* AUTOMATIC publish: select all in the realized view */
  gtk_text_buffer_get_bounds(GTK_TEXT_BUFFER(src), &s, &e);
  gtk_text_buffer_select_range(GTK_TEXT_BUFFER(src), &s, &e);
  pump(300);
  g_print("  PRIMARY holds GtkTextBuffer: %s\n",
     gdk_content_formats_contain_gtype(gdk_clipboard_get_formats(pri), GTK_TYPE_TEXT_BUFFER)
     ? "YES" : "no");

  /* paste into a fresh buffer with NO repair hook -- master's state */
  GtkSourceBuffer *dst = gtk_source_buffer_new(NULL);
  g_signal_connect(dst, "insert-text", G_CALLBACK(on_insert), NULL);
  g_print("  emissions during PRIMARY paste (observer only, no repair):\n");
  gtk_text_buffer_paste_clipboard(GTK_TEXT_BUFFER(dst), pri, NULL, TRUE);
  pump(900);

  GtkTextIter a,z; gtk_text_buffer_get_bounds(GTK_TEXT_BUFFER(dst), &a, &z);
  char *got = gtk_text_buffer_get_text(GTK_TEXT_BUFFER(dst), &a, &z, TRUE);
  hexp("expected", DOC);
  hexp("actual", got);
  g_print("  emissions: %d %s\n", emissions,
          emissions > 1 ? "(multi-run: the split IS present)" : "(single run)");
  g_print("  VERDICT: %s\n", strcmp(got, DOC)==0 ? "byte-identical -- PRIMARY is CLEAN with no hook"
                                                 : "*** CORRUPTED ***");
  g_free(got);
  g_main_loop_quit(loop);
  return G_SOURCE_REMOVE;
}

int main(void) {
  gtk_init(); gtk_source_init();
  GtkSourceLanguageManager *lm = gtk_source_language_manager_get_default();
  src = gtk_source_buffer_new_with_language(gtk_source_language_manager_get_language(lm, "markdown"));
  gtk_source_buffer_set_highlight_syntax(src, TRUE);
  gtk_text_buffer_set_text(GTK_TEXT_BUFFER(src), DOC, -1);
  view = gtk_text_view_new_with_buffer(GTK_TEXT_BUFFER(src));
  GtkWidget *win = gtk_window_new();
  gtk_window_set_child(GTK_WINDOW(win), view);
  gtk_window_present(GTK_WINDOW(win));
  pump(600);
  g_print("backend: %s / view realized: %s\n",
    G_OBJECT_TYPE_NAME(gdk_display_get_default()),
    gtk_widget_get_realized(view) ? "YES" : "no");
  loop = g_main_loop_new(NULL, FALSE);
  g_idle_add(go, NULL);
  g_main_loop_run(loop);
  return 0;
}
