/* Does gtk4-rs's connect_insert_text trampoline -- which hands the closure a COPY
   of the caller's GtkTextIter and never writes it back -- explain the diagnostics,
   independently of the backend? Both shapes, one binary, same buffer, same payload.
   Researcher measured this at GTK 4.6.9 / X11; this is the 4.22.4 / Quartz leg. */
#include <gtk/gtk.h>
#include <string.h>

static int rust_shape = 0, emissions = 0;

static void hexp(const char *l, const char *s) {
  g_print("    %-8s ", l);
  for (const unsigned char *p=(const unsigned char*)s; *p; p++) g_print("%02x ", *p);
  g_print("\n");
}

static void on_insert(GtkTextBuffer *b, GtkTextIter *it, char *text, int len, gpointer u) {
  static int busy = 0;
  if (busy) return;
  emissions++;
  int lone = 0;
  for (int i=0;i<len;i++) if (text[i]=='\r' && (i+1>=len||text[i+1]!='\n')) lone=1;
  if (!lone) return;
  char *fixed = g_strndup(text, len);
  for (int i=0;i<len;i++) if (fixed[i]=='\r' && (i+1>=len||fixed[i+1]!='\n')) fixed[i]='\n';

  g_signal_stop_emission_by_name(b, "insert-text");
  busy = 1;
  if (rust_shape) {
    /* gtk4-rs: closure receives a COPY (from_glib_none), never written back. */
    GtkTextIter copy = *it;
    gtk_text_buffer_insert(b, &copy, fixed, len);
    /* deliberately NOT: *it = copy; */
  } else {
    /* C: the handler gets the caller's iter by pointer; insert fixes it in place. */
    gtk_text_buffer_insert(b, it, fixed, len);
  }
  busy = 0;
  g_free(fixed);
}

/* "ab\r\ncd\r\nef\r\ngh" with a tag ending at each \n, so insert_range chunks
   the CRLF pairs apart -- the multi-run condition, asserted below. */
static const char *PAYLOAD = "ab\r\ncd\r\nef\r\ngh";

int main(int argc, char **argv) {
  gtk_init();
  g_print("GTK %d.%d.%d / %s\n", gtk_get_major_version(), gtk_get_minor_version(),
          gtk_get_micro_version(), G_OBJECT_TYPE_NAME(gdk_display_get_default()));

  for (int shape = 0; shape < 2; shape++) {
    rust_shape = shape; emissions = 0;
    /* ONE tag table, shared -- gtk_text_buffer_insert_range requires it, and it is
       what create_clipboard_contents_buffer does for a same-app copy. */
    GtkTextTagTable *table = gtk_text_tag_table_new();
    GtkTextBuffer *src = gtk_text_buffer_new(table);
    gtk_text_buffer_set_text(src, PAYLOAD, -1);
    GtkTextTag *t = gtk_text_buffer_create_tag(src, NULL, NULL);
    GtkTextIter a, z;
    int spans[3][2] = { {0,3}, {4,7}, {8,11} };   /* toggles land on each \n */
    for (int i=0;i<3;i++) {
      gtk_text_buffer_get_iter_at_offset(src, &a, spans[i][0]);
      gtk_text_buffer_get_iter_at_offset(src, &z, spans[i][1]);
      gtk_text_buffer_apply_tag(src, t, &a, &z);
    }
    GtkTextBuffer *dst = gtk_text_buffer_new(table);
    g_signal_connect(dst, "insert-text", G_CALLBACK(on_insert), NULL);
    GtkTextIter di; gtk_text_buffer_get_start_iter(dst, &di);
    gtk_text_buffer_get_bounds(src, &a, &z);
    gtk_text_buffer_insert_range(dst, &di, &a, &z);

    GtkTextIter s2, e2; gtk_text_buffer_get_bounds(dst, &s2, &e2);
    char *got = gtk_text_buffer_get_text(dst, &s2, &e2, TRUE);
    g_print("\n  == %s ==\n", shape ? "gtk4-rs shape (COPY, no write-back)"
                                    : "C shape (signal's own iter)");
    g_print("    emissions: %d %s\n", emissions,
            emissions > 1 ? "(multi-run precondition MET)" : "*** SINGLE RUN - rig broken ***");
    hexp("expected", PAYLOAD);
    hexp("actual", got);
    g_print("    %s\n", strcmp(got, PAYLOAD)==0 ? "byte-identical" : "*** DIVERGED ***");
    g_free(got); g_object_unref(src); g_object_unref(dst);
  }
  return 0;
}
