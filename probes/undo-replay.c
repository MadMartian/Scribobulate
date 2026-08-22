/* Does undo replay run through an insert-text handler, and does it diverge?
   Researcher measured this at GTK 4.6.9; this is the 4.22.4 / Quartz leg. */
#include <gtk/gtk.h>
#include <gtksourceview/gtksource.h>
#include <string.h>

static int hook_fires = 0, in_replay = 0, use_guard = 0, in_repair = 0;

static void hexp(const char *l, const char *s) {
  g_print("  %-22s ", l);
  for (const unsigned char *p=(const unsigned char*)s; *p; p++) g_print("%02x ", *p);
  g_print("\n");
}
static gboolean has_lone_cr(const char *t, int n) {
  for (int i=0;i<n;i++) if (t[i]=='\r' && (i+1>=n || t[i+1]!='\n')) return TRUE;
  return FALSE;
}
static void on_insert(GtkTextBuffer *b, GtkTextIter *it, char *text, int len, gpointer u) {
  if (in_repair) return;
  if (use_guard && in_replay) return;              /* researcher's proposed bracket */
  if (!has_lone_cr(text, len)) return;
  hook_fires++;
  char *fixed = g_strndup(text, len);
  for (int i=0;i<len;i++) if (fixed[i]=='\r' && (i+1>=len||fixed[i+1]!='\n')) fixed[i]='\n';
  g_signal_stop_emission_by_name(b, "insert-text");
  in_repair = 1; gtk_text_buffer_insert(b, it, fixed, len); in_repair = 0;
  g_free(fixed);
}
static void undo_before(GtkTextBuffer *b, gpointer u){ in_replay = 1; }
static void undo_after (GtkTextBuffer *b, gpointer u){ in_replay = 0; }

static void run(const char *label, int guard) {
  use_guard = guard; hook_fires = 0; in_replay = 0;
  GtkSourceBuffer *sb = gtk_source_buffer_new(NULL);
  GtkTextBuffer *b = GTK_TEXT_BUFFER(sb);

  /* A legacy document already holding a lone CR, populated BEFORE the hook is
     armed -- researcher's stated precondition, and one line of ordering away. */
  gtk_text_buffer_set_text(b, "x\ry", -1);
  gtk_text_buffer_set_enable_undo(b, TRUE);

  g_signal_connect(b, "insert-text", G_CALLBACK(on_insert), NULL);
  if (guard) {
    g_signal_connect(b, "undo", G_CALLBACK(undo_before), NULL);
    g_signal_connect_after(b, "undo", G_CALLBACK(undo_after), NULL);
  }

  GtkTextIter s,e; gtk_text_buffer_get_bounds(b,&s,&e);
  char *before = gtk_text_buffer_get_text(b,&s,&e,TRUE);

  /* delete the whole thing, then undo it */
  gtk_text_buffer_begin_user_action(b);
  gtk_text_buffer_get_bounds(b,&s,&e);
  gtk_text_buffer_delete(b,&s,&e);
  gtk_text_buffer_end_user_action(b);
  gtk_text_buffer_undo(b);

  gtk_text_buffer_get_bounds(b,&s,&e);
  char *after = gtk_text_buffer_get_text(b,&s,&e,TRUE);
  g_print("== %s (guard=%s) ==\n", label, guard?"ON":"off");
  hexp("before delete", before);
  hexp("after undo", after);
  g_print("  hook fired during replay: %d\n  VERDICT: %s\n\n",
          hook_fires, strcmp(before,after)==0 ? "undo restored the original"
                                              : "*** UNDO DIVERGED ***");
  g_free(before); g_free(after); g_object_unref(sb);
}

int main(void) {
  gtk_init(); gtk_source_init();
  /* Every component MEASURED at runtime. This line used to hardcode
   * "GtkSourceView 5.20.0" beside two versions it really did read, so a probe
   * run against a different GtkSourceView would have reported the version the
   * author had when they wrote it -- a banner that looks like evidence and is
   * not, in the one place a reader checks what the result applies to. */
  g_print("GTK %d.%d.%d / GtkSourceView %u.%u.%u / %s\n\n",
    gtk_get_major_version(), gtk_get_minor_version(), gtk_get_micro_version(),
    gtk_source_get_major_version(), gtk_source_get_minor_version(),
    gtk_source_get_micro_version(),
    G_OBJECT_TYPE_NAME(gdk_display_get_default()));
  run("undo replay through the hook", 0);
  run("undo replay with the bracket", 1);
  return 0;
}
