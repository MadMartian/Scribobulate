#include <gtk/gtk.h>
#include <gtksourceview/gtksource.h>
#include <string.h>

/* ---- the two documents. CRLF throughout; one fence closed, one not. ---- */
static const char *DOC_OPEN  = "```rust\r\nfn main() {}\r\n";
static const char *DOC_CLOSED= "```rust\r\nfn main() {}\r\n```\r\n";

static GMainLoop *loop;
static int  apply_repair = 0;     /* emulate lineendings::wire_paste_normalization */
static int  run_no = 0;
static GString *run_log;

static void hex(GString *o, const char *s, gssize len) {
  if (len < 0) len = strlen(s);
  for (gssize i = 0; i < len; i++)
    g_string_append_printf(o, "%02x ", (unsigned char)s[i]);
}

static char *hexdup(const char *s, gssize len) {
  GString *o = g_string_new(NULL);
  hex(o, s, len);
  return g_string_free(o, FALSE);
}

/* ---- the handler under test: shaped exactly like the shipped one ---- */
static gboolean in_repair = FALSE;

static gboolean has_lone_cr(const char *t, int len) {
  for (int i = 0; i < len; i++)
    if (t[i] == '\r' && (i + 1 >= len || t[i+1] != '\n')) return TRUE;
  return FALSE;
}

static void on_insert(GtkTextBuffer *b, GtkTextIter *loc, char *text, int len, gpointer u) {
  char *h = hexdup(text, len);
  g_string_append_printf(run_log, "    run %d  off=%-4d len=%-3d  %s\n",
                         ++run_no, gtk_text_iter_get_offset(loc), len, h);
  g_free(h);

  if (!apply_repair || in_repair) return;
  if (!has_lone_cr(text, len)) return;

  char *fixed = g_strndup(text, len);
  for (int i = 0; i < len; i++)
    if (fixed[i] == '\r' && (i + 1 >= len || fixed[i+1] != '\n')) fixed[i] = '\n';
  g_string_append_printf(run_log, "      ^ handler REWROTE this run\n");
  g_signal_stop_emission_by_name(b, "insert-text");
  in_repair = TRUE;
  gtk_text_buffer_insert(b, loc, fixed, len);
  in_repair = FALSE;
  g_free(fixed);
}

/* ---- helpers ---- */
static GtkSourceBuffer *make_source_buffer(const char *doc) {
  GtkSourceLanguageManager *lm = gtk_source_language_manager_get_default();
  GtkSourceLanguage *lang = gtk_source_language_manager_get_language(lm, "markdown");
  GtkSourceBuffer *b = gtk_source_buffer_new_with_language(lang);
  gtk_source_buffer_set_highlight_syntax(b, TRUE);
  gtk_text_buffer_set_text(GTK_TEXT_BUFFER(b), doc, -1);
  GtkTextIter s, e;
  gtk_text_buffer_get_bounds(GTK_TEXT_BUFFER(b), &s, &e);
  gtk_source_buffer_ensure_highlight(b, &s, &e);
  return b;
}

static char *buffer_bytes(GtkTextBuffer *b) {
  GtkTextIter s, e;
  gtk_text_buffer_get_bounds(b, &s, &e);
  return gtk_text_buffer_get_text(b, &s, &e, TRUE);
}

/* Requirement 2: enumerate tag toggles, flag any sitting on \n preceded by \r. */
static int report_toggles(GtkTextBuffer *b, const char *label) {
  char *txt = buffer_bytes(b);
  g_print("  [%s] tag-toggle offsets:\n", label);
  GtkTextIter it;
  gtk_text_buffer_get_start_iter(b, &it);
  int dangerous = 0;
  while (gtk_text_iter_forward_to_tag_toggle(&it, NULL)) {
    int off = gtk_text_iter_get_offset(&it);
    /* offset is in CHARS; this doc is ASCII so chars == bytes */
    char here = txt[off] ? txt[off] : 0;
    char prev = off > 0 ? txt[off-1] : 0;
    gboolean bad = (here == '\n' && prev == '\r');
    GSList *on  = gtk_text_iter_get_toggled_tags(&it, TRUE);
    GSList *off_= gtk_text_iter_get_toggled_tags(&it, FALSE);
    GString *names = g_string_new(NULL);
    for (GSList *l = on;  l; l = l->next) { char *n=NULL; g_object_get(l->data,"name",&n,NULL); g_string_append_printf(names,"+%s ", n?n:"(anon)"); g_free(n); }
    for (GSList *l = off_;l; l = l->next) { char *n=NULL; g_object_get(l->data,"name",&n,NULL); g_string_append_printf(names,"-%s ", n?n:"(anon)"); g_free(n); }
    g_slist_free(on); g_slist_free(off_);
    g_print("      off=%-4d prev=%02x here=%02x %s %s\n", off,
            (unsigned char)prev, (unsigned char)here, names->str,
            bad ? "  <<< SPLITS A CRLF" : "");
    if (bad) dangerous++;
    g_string_free(names, TRUE);
  }
  if (!dangerous) g_print("      (no toggle sits between a CR and its LF)\n");
  g_free(txt);
  return dangerous;
}

typedef struct { const char *label; const char *doc; gboolean primary; gboolean plaintext; } Case;
static Case  cases[8];
static int   ncases = 0, cur = 0;
static gboolean any_corruption = FALSE;

static gboolean finish_case(gpointer data);

static void run_case(void) {
  if (cur >= ncases) { g_main_loop_quit(loop); return; }
  Case *c = &cases[cur];
  g_print("\n=== %s  (repair=%s, clipboard=%s) ===\n",
          c->label, apply_repair ? "ON" : "off", c->primary ? "PRIMARY" : "CLIPBOARD");

  GtkSourceBuffer *src = make_source_buffer(c->doc);
  int bad = report_toggles(GTK_TEXT_BUFFER(src), "source");

  GdkDisplay *d = gdk_display_get_default();
  GdkClipboard *cb = c->primary ? gdk_display_get_primary_clipboard(d)
                                : gdk_display_get_clipboard(d);

  GtkTextIter s, e;
  gtk_text_buffer_get_bounds(GTK_TEXT_BUFFER(src), &s, &e);
  if (c->plaintext) {
    /* researcher's proposal: publish PLAIN TEXT, not a GtkTextBuffer */
    char *txt = buffer_bytes(GTK_TEXT_BUFFER(src));
    gdk_clipboard_set_text(cb, txt);
    g_free(txt);
  } else if (c->primary) {
    /* PRIMARY: GtkTextView does this at realize; selecting is what publishes. */
    gtk_text_buffer_add_selection_clipboard(GTK_TEXT_BUFFER(src), cb);
    gtk_text_buffer_select_range(GTK_TEXT_BUFFER(src), &s, &e);
  } else {
    gtk_text_buffer_select_range(GTK_TEXT_BUFFER(src), &s, &e);
    gtk_text_buffer_copy_clipboard(GTK_TEXT_BUFFER(src), cb);
  }

  /* paste into a fresh markdown buffer with the handler installed */
  GtkSourceBuffer *dst = make_source_buffer("");
  run_no = 0;
  g_string_truncate(run_log, 0);
  g_signal_connect(dst, "insert-text", G_CALLBACK(on_insert), NULL);
  gtk_text_buffer_paste_clipboard(GTK_TEXT_BUFFER(dst), cb, NULL, TRUE);

  g_object_set_data(G_OBJECT(dst), "case-bad", GINT_TO_POINTER(bad));
  g_timeout_add(900, finish_case, dst);
  g_object_unref(src);
}

static gboolean finish_case(gpointer data) {
  GtkSourceBuffer *dst = data;
  Case *c = &cases[cur];
  g_print("  insert-text emissions during paste:\n%s", run_log->str);

  char *got = buffer_bytes(GTK_TEXT_BUFFER(dst));
  /* Requirement 1: BYTE-WISE diff, never a length or char-count compare. */
  gsize wl = strlen(c->doc), gl = strlen(got);
  gboolean identical = (wl == gl) && memcmp(c->doc, got, wl) == 0;
  char *hw = hexdup(c->doc, wl), *hg = hexdup(got, gl);
  g_print("  expected %s\n  actual   %s\n", hw, hg);
  g_print("  VERDICT: %s%s\n",
          identical ? "byte-identical" : "*** CORRUPTED ***",
          (!identical && wl == gl) ? "  (SAME LENGTH — a char-count check would have passed)" : "");
  if (!identical) any_corruption = TRUE;
  g_free(hw); g_free(hg); g_free(got);
  g_object_unref(dst);
  cur++;
  run_case();
  return G_SOURCE_REMOVE;
}

static gboolean kick(gpointer u) { run_case(); return G_SOURCE_REMOVE; }

int main(int argc, char **argv) {
  gtk_init();
  gtk_source_init();
  apply_repair = (argc > 1 && g_str_equal(argv[1], "repair"));
  run_log = g_string_new(NULL);

  g_print("GTK %d.%d.%d / GtkSourceView %s / backend %s\n",
          gtk_get_major_version(), gtk_get_minor_version(), gtk_get_micro_version(),
          "5.20.0", G_OBJECT_TYPE_NAME(gdk_display_get_default()));

  cases[ncases++] = (Case){ "UNCLOSED fence (researcher's repro)", DOC_OPEN,   FALSE, FALSE };
  cases[ncases++] = (Case){ "CLOSED fence (control)",              DOC_CLOSED, FALSE, FALSE };
  cases[ncases++] = (Case){ "UNCLOSED fence via PRIMARY",          DOC_OPEN,   TRUE,  FALSE };
  cases[ncases++] = (Case){ "CLOSED fence via PRIMARY (control)",  DOC_CLOSED, TRUE,  FALSE };
  cases[ncases++] = (Case){ "UNCLOSED fence, PLAIN-TEXT publication", DOC_OPEN, FALSE, TRUE };

  loop = g_main_loop_new(NULL, FALSE);
  g_idle_add(kick, NULL);
  g_main_loop_run(loop);
  g_print("\n==== OVERALL: %s ====\n", any_corruption ? "CORRUPTION REPRODUCED" : "no corruption observed");
  return any_corruption ? 1 : 0;
}
