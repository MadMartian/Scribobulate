/* Which HANDLER SHAPE produces the two diagnostics linux saw in-tree?
   Researcher measured three arms at GTK 4.6.9/X11 and predicts they follow the
   CODE, not the backend. This is the 4.22.4 / Quartz leg of that prediction. */
#include <gtk/gtk.h>
#include <string.h>

static int arm = 0;

static void hexp(const char *l, const char *s) {
  g_print("  %-10s ", l);
  for (const unsigned char *p=(const unsigned char*)s; *p; p++) g_print("%02x ", *p);
  g_print("\n");
}

static GtkTextBuffer *other = NULL;

static void on_insert(GtkTextBuffer *b, GtkTextIter *it, char *text, int len, gpointer u) {
  static int busy = 0;
  if (busy) return;
  int lone = 0;
  for (int i=0;i<len;i++) if (text[i]=='\r' && (i+1>=len||text[i+1]!='\n')) lone=1;
  if (!lone) return;

  char *fixed = g_strndup(text, len);
  for (int i=0;i<len;i++) if (fixed[i]=='\r' && (i+1>=len||fixed[i+1]!='\n')) fixed[i]='\n';
  g_signal_stop_emission_by_name(b, "insert-text");
  busy = 1;
  if (arm == 0) {
    /* A: insert at the handler's own iter */
    gtk_text_buffer_insert(b, it, fixed, len);
  } else if (arm == 1) {
    /* B: insert at an iter belonging to a DIFFERENT buffer */
    GtkTextIter foreign;
    gtk_text_buffer_get_start_iter(other, &foreign);
    gtk_text_buffer_insert(b, &foreign, fixed, len);
  } else {
    /* C: capture an iter, mutate the buffer, then reuse the captured iter */
    GtkTextIter captured = *it;
    gtk_text_buffer_insert(b, it, "Z", 1);          /* mutation invalidates `captured` */
    gtk_text_buffer_insert(b, &captured, fixed, len);
  }
  busy = 0;
  g_free(fixed);
}

int main(int argc, char **argv) {
  gtk_init();
  const char *names[] = { "A own-iter", "B foreign-iter", "C stale-iter" };
  g_print("GTK %d.%d.%d / %s\n\n", gtk_get_major_version(), gtk_get_minor_version(),
          gtk_get_micro_version(), G_OBJECT_TYPE_NAME(gdk_display_get_default()));
  arm = atoi(argv[1]);
  other = gtk_text_buffer_new(NULL);
  GtkTextBuffer *b = gtk_text_buffer_new(NULL);
  g_signal_connect(b, "insert-text", G_CALLBACK(on_insert), NULL);
  gtk_text_buffer_set_text(b, "", -1);
  GtkTextIter s; gtk_text_buffer_get_start_iter(b, &s);
  gtk_text_buffer_insert(b, &s, "x\ry", 3);
  GtkTextIter a,e; gtk_text_buffer_get_bounds(b,&a,&e);
  char *got = gtk_text_buffer_get_text(b,&a,&e,TRUE);
  g_print("== arm %s ==\n", names[arm]);
  hexp("result", got);
  return 0;
}
