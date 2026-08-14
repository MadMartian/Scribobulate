# The binary-to-licence table for the Windows installer.
#
# WHAT THIS IS FOR. packaging\windows\stage.ps1 assembles a tree of 863 files, of
# which exactly two are ours. Everything else is somebody else's software carrying
# somebody else's terms, and the installer currently ships not one line of their
# licence text. This file names, for every staged file, which upstream project it
# belongs to, under what licence, and WHERE THE TEXT OF THAT LICENCE COMES FROM.
#
# It is data, not documentation: packaging\windows\verify-licenses.ps1 reads it and
# fails the build on any disagreement with the staged tree, in BOTH directions. A
# table nobody executes drifts from the artefact within one dependency change, and
# the drift is invisible because the table still reads plausibly.
#
# FIELDS
#   Id        stable key, used in gate output
#   Component the upstream project, named as its own build names it
#   Match     .NET regex over the staged path, relative to the stage root, with
#             backslash separators. Must match at least one staged file, and no
#             staged file may match two rows.
#   License   SPDX expression
#   Source    where the licence TEXT comes from:
#               prefix:<rel>  a file in the gvsbuild prefix we stage from
#               repo:<rel>    a file vendored into this repository
#   Expect    a literal string that MUST occur in the Source file. This is the
#             condition that separates "a licence file exists" from "the licence
#             is there", and it is not hypothetical -- see the pcre2, cairo and
#             gettext rows below, all three of which have a Source file that
#             exists and does not contain the licence it is supposed to carry.
#   Evidence  M = measured on this box from the artefact itself
#             I = inferred; Basis says from what
#   Basis     what the mark rests on, in one sentence
#
# ROWS ARE RED ON PURPOSE. Several Source paths do not exist yet. That is the
# finding, not a defect in the table: gvsbuild ships no licence text at all for
# freetype, graphene and libxml2 (their share\doc directories are present and
# EMPTY -- measured, 0 files each), ships 366 files of Sphinx manual for tiff
# without a licence among them, and ships no text for the MSVC runtime because
# that lives in the Visual Studio EULA rather than in the redist directory.

@{
    Rows = @(

        # -- our own code ----------------------------------------------------

        @{
            Id        = 'scribobulate'
            Component = 'Scribobulate'
            Match     = '^bin\\scribobulate\.exe$'
            License   = 'Apache-2.0'
            Source    = 'repo:LICENSE'
            Expect    = 'Apache License'
            Evidence  = 'M'
            Basis     = 'VersionInfo ProductName on the staged executable reads "Scribobulate".'
        },
        @{
            # NOT STAGED TODAY. The About dialog tells the user this file is "in the
            # distribution"; the distribution does not contain it. Kept in the table
            # so the gate says so on every run rather than waiting to be noticed.
            Id        = 'scribobulate-license-text'
            Component = 'Scribobulate (licence text)'
            Match     = '^LICENSE$'
            License   = 'Apache-2.0'
            Source    = 'repo:LICENSE'
            Expect    = 'Apache License'
            Evidence  = 'M'
            Basis     = 'The file exists in the repository and is absent from the staged tree.'
        },
        @{
            # The syntect/two-face syntax grammars are compiled INTO scribobulate.exe
            # and leave no file of their own, which is the same mechanism that hid
            # librsvg's Rust graph. Statically linked, so this row is not
            # Windows-specific -- the deb, rpm and .app owe it too.
            Id        = 'syntax-grammars'
            Component = 'Syntax grammars (two-face / syntect assets, statically linked)'
            Match     = '^THIRD-PARTY-LICENSES\.md$'
            License   = 'MIT AND Apache-2.0 AND BSD-2-Clause AND BSD-3-Clause'
            Source    = 'repo:THIRD-PARTY-LICENSES.md'
            Expect    = 'Permission is hereby granted'
            Evidence  = 'M'
            Basis     = 'Generated from two_face::acknowledgement::listing(); the file states its own provenance.'
        },

        # -- the GTK stack ---------------------------------------------------

        @{
            Id        = 'glib'
            Component = 'GLib'
            Match     = '^bin\\(glib-2\.0-0|gobject-2\.0-0|gio-2\.0-0|gmodule-2\.0-0)\.dll$|^bin\\gdbus\.exe$'
            License   = 'LGPL-2.1-or-later'
            Source    = 'prefix:share\doc\glib\LGPL-2.1-or-later.txt'
            Expect    = 'GNU LESSER GENERAL PUBLIC LICENSE'
            Evidence  = 'M'
            Basis     = 'VersionInfo ProductName on all four staged DLLs reads "GLib".'
        },
        @{
            Id        = 'gtk4'
            Component = 'GTK'
            Match     = '^bin\\gtk-4-1\.dll$'
            License   = 'LGPL-2.0-or-later'
            Source    = 'prefix:share\doc\gtk4\COPYING'
            Expect    = 'GNU LIBRARY GENERAL PUBLIC LICENSE'
            Evidence  = 'M'
            Basis     = 'VersionInfo ProductName reads "GTK+"; the shipped COPYING is the Library GPL v2, NOT v2.1.'
        },
        @{
            # MEASURED, and it corrects an assumption. gschemas.compiled is an
            # aggregate and could have carried schemas from any installed project;
            # every schema id inside it is org.gtk.gtk4.*, so it carries GTK's and
            # nothing else's.
            Id        = 'gtk4-schemas'
            Component = 'GTK (compiled GSettings schemas)'
            Match     = '^share\\glib-2\.0\\schemas\\gschemas\.compiled$'
            License   = 'LGPL-2.0-or-later'
            Source    = 'prefix:share\doc\gtk4\COPYING'
            Expect    = 'GNU LIBRARY GENERAL PUBLIC LICENSE'
            Evidence  = 'M'
            Basis     = 'Every schema id in the compiled blob is org.gtk.gtk4.*; no glib or gtksourceview schema is present.'
        },
        @{
            Id        = 'gdk-pixbuf'
            Component = 'gdk-pixbuf'
            Match     = '^bin\\gdk_pixbuf-2\.0-0\.dll$|^lib\\gdk-pixbuf-2\.0\\2\.10\.0\\loaders\.cache$'
            License   = 'LGPL-2.1-or-later'
            Source    = 'prefix:share\doc\gdk-pixbuf\COPYING'
            Expect    = 'GNU LESSER GENERAL PUBLIC LICENSE'
            Evidence  = 'M'
            Basis     = 'VersionInfo ProductName reads "GTK+"; the loaders.cache is gdk-pixbuf-query-loaders output.'
        },
        @{
            Id        = 'gtksourceview'
            Component = 'GtkSourceView'
            Match     = '^bin\\gtksourceview-5-0\.dll$|^share\\gtksourceview-5\\'
            License   = 'LGPL-2.1-or-later'
            Source    = 'prefix:share\doc\gtksourceview5\COPYING'
            Expect    = 'GNU LESSER GENERAL PUBLIC LICENSE'
            Evidence  = 'I'
            Basis     = 'No VersionInfo; mapped by the gtksourceview-5 pkg-config module and share\doc\gtksourceview5 in the same prefix.'
        },
        @{
            # THE MOST-CORRECTED ROW IN THIS TABLE, and each pass was wrong about a
            # different thing. First the PROJECT: these 15 SVGs were attributed to
            # adwaita-icon-theme, and they are GtkSourceView's completion-provider
            # icons -- lang-class, lang-enum, completion-snippet. MEASURED: the
            # hicolor and Adwaita SVG name sets are DISJOINT (0 of 15 shared) and no
            # lang-* or completion-* file exists among Adwaita's 714. A count of the
            # files agreed with the prefix and still named the wrong project, because
            # a count cannot see a name.
            #
            # Then the LICENCE, in the other direction. Having established the
            # project, the obvious next step was the project's own licence --
            # LGPL-2.1, since the icons carried no per-file header and no COPYING sat
            # beside them. Wrong: GtkSourceView's data/icons/COPYING says CC-BY-SA-3.0
            # and governs exactly this tree. It is not in the prefix because
            # data/icons/meson.build runs install_subdir('hicolor', ...) and installs
            # no COPYING at all -- so the governing file has never existed in ANY
            # installed tree on ANY platform.
            #
            # The absence of a declaration in an installed tree is evidence about the
            # INSTALL RULE, not about the terms. That is why the Source below can
            # never be a prefix: path, and why this row wants a CREDITS.txt: CC-BY-SA
            # 3.0 s4(c) obliges an attribution that no upstream file supplies.
            Id        = 'gtksourceview-icons'
            Component = 'GtkSourceView (completion icons, artwork)'
            Match     = '^share\\icons\\hicolor\\scalable\\actions\\.+\.svg$'
            License   = 'CC-BY-SA-3.0'
            Source    = @(
                'repo:packaging\windows\licenses\gtksourceview-icons\CC-BY-SA-3.0.txt',
                'repo:packaging\windows\licenses\gtksourceview-icons\CREDITS.txt'
            )
            Expect    = @(
                'Creative Commons Legal Code',
                'Christian Hergert'
            )
            Evidence  = 'M'
            Basis     = 'GtkSourceView 5.20.0 data/icons/COPYING reads "The icons here are licensed under the CC-by-SA 3." and covers this exact set; root COPYING is LGPL-2.1 and governs code, not artwork.'
        },
        @{
            Id        = 'adwaita-icon-theme'
            Component = 'adwaita-icon-theme'
            # The negative lookahead is not tidiness. Without it this row and
            # icon-theme-cache both claim share\icons\Adwaita\icon-theme.cache, and
            # the file is then attributed twice under two different licences, with
            # which one a reader believes decided by table order. The gate caught it.
            Match     = '^share\\icons\\Adwaita\\(?!icon-theme\.cache$)'
            License   = 'CC-BY-SA-3.0 OR LGPL-3.0-or-later'
            Source    = 'prefix:share\doc\adwaita-icon-theme\COPYING_CCBYSA3'
            Expect    = 'Attribution-Share Alike 3.0'
            Evidence  = 'M'
            Basis     = 'Staged from share\icons\Adwaita of the prefix that also ships this COPYING_CCBYSA3.'
        },
        @{
            Id        = 'hicolor-icon-theme'
            Component = 'hicolor-icon-theme'
            Match     = '^share\\icons\\hicolor\\index\.theme$'
            License   = 'GPL-2.0-or-later'
            Source    = 'repo:packaging\windows\licenses\hicolor-icon-theme\COPYING'
            Expect    = 'GNU GENERAL PUBLIC LICENSE'
            Evidence  = 'M'
            Basis     = 'Taken from the hicolor-icon-theme 0.18 source tree gvsbuild built (meson.build declares version 0.18): its own COPYING is the full GPL-2 text, and its index.theme is byte-identical to both the prefix''s and the staged one (SHA-256 A02DB5E1...CB9BC5), so this is the package that produced the staged file rather than a filename match. The prefix installs no COPYING, which is why it is vendored. CAVEAT ON "-or-later": upstream ships NO version-selection statement anywhere in that tree -- no per-file header, no notice beside index.theme, no copyright line at all (measured). The -or-later half is Debian''s determination, not measured here; only the licence identity is.'
        },
        @{
            Id        = 'icon-theme-cache'
            Component = 'GTK (gtk-update-icon-cache output)'
            Match     = '^share\\icons\\(Adwaita|hicolor)\\icon-theme\.cache$'
            License   = 'LGPL-2.0-or-later'
            Source    = 'prefix:share\doc\gtk4\COPYING'
            Expect    = 'GNU LIBRARY GENERAL PUBLIC LICENSE'
            Evidence  = 'I'
            Basis     = 'A generated index, produced by GTK''s gtk-update-icon-cache rather than shipped by either theme.'
        },
        @{
            Id        = 'pango'
            Component = 'Pango'
            Match     = '^bin\\pango(cairo|ft2|win32)?-1\.0-0\.dll$'
            License   = 'LGPL-2.0-or-later'
            Source    = 'prefix:share\doc\pango\COPYING'
            Expect    = 'GNU LIBRARY GENERAL PUBLIC LICENSE'
            Evidence  = 'M'
            Basis     = 'VersionInfo ProductName on all four reads Pango / PangoCairo / PangoFT2 / PangoWin32.'
        },
        @{
            Id        = 'graphene'
            Component = 'Graphene'
            Match     = '^bin\\graphene-1\.0-0\.dll$'
            License   = 'MIT'
            Source    = 'repo:packaging\windows\licenses\graphene\LICENSE.txt'
            Expect    = 'Permission is hereby granted'
            Evidence  = 'I'
            Basis     = 'Mapped by the graphene-1.0 pkg-config module; share\doc\graphene exists in the prefix and is EMPTY (0 files), so the text must be vendored.'
        },
        @{
            Id        = 'librsvg'
            Component = 'librsvg'
            Match     = '^bin\\rsvg-2-2\.dll$|^lib\\gdk-pixbuf-2\.0\\2\.10\.0\\loaders\\pixbufloader_svg\.dll$'
            License   = 'LGPL-2.1-or-later'
            Source    = 'prefix:share\doc\librsvg\COPYING.LIB'
            Expect    = 'GNU LESSER GENERAL PUBLIC LICENSE'
            Evidence  = 'I'
            Basis     = 'Mapped by the librsvg-2.0 pkg-config module; pixbufloader_svg is librsvg''s gdk-pixbuf loader, not gdk-pixbuf''s.'
        },
        @{
            # The second obligation for one binary. COPYING.LIB covers librsvg; the
            # 198 Rust crates statically linked into rsvg-2-2.dll leave no file in
            # the tree and are covered by nothing else here.
            Id        = 'librsvg-rust'
            Component = 'librsvg (statically linked Rust crates)'
            Match     = '^share\\licenses\\librsvg\\THIRD-PARTY-RUST-NOTICES\.txt$'
            License   = 'MIT AND Apache-2.0 AND MPL-2.0 AND Unicode-3.0 AND BSD-3-Clause'
            Source    = 'repo:packaging\windows\licenses\librsvg\THIRD-PARTY-RUST-NOTICES.txt'
            Expect    = 'THIRD-PARTY RUST NOTICES for librsvg'
            Evidence  = 'M'
            Basis     = 'Generated by cargo-about from librsvg''s own Cargo.lock, dev-dependencies excluded by name.'
        },

        # -- the rendering and text stack ------------------------------------

        @{
            # THREE Sources and three Expects. The prefix's COPYING is cairo's own
            # summary of its dual licensing and names two texts the prefix does not
            # contain; vendoring the summary alone would ship a document that points
            # at nothing. Both texts ship, so no election is made on cairo's behalf.
            Id        = 'cairo'
            Component = 'cairo'
            Match     = '^bin\\cairo(-gobject-2|-script-interpreter-2|-2)\.dll$'
            License   = 'LGPL-2.1-only OR MPL-1.1'
            Source    = @(
                'repo:packaging\windows\licenses\cairo\COPYING',
                'repo:packaging\windows\licenses\cairo\COPYING-LGPL-2.1',
                'repo:packaging\windows\licenses\cairo\COPYING-MPL-1.1'
            )
            Expect    = @(
                'Cairo is free software',
                'GNU LESSER GENERAL PUBLIC LICENSE',
                'Mozilla Public License Version 1.1'
            )
            Evidence  = 'I'
            Basis     = 'Mapped by the cairo pkg-config module, version 1.18.4. See packaging\windows\licenses\PROVENANCE.md.'
        },
        @{
            Id        = 'freetype'
            Component = 'FreeType'
            Match     = '^bin\\freetype-6\.dll$'
            License   = 'FTL'
            Source    = @('repo:packaging\windows\licenses\freetype\FTL.TXT',
                          'repo:packaging\windows\licenses\freetype\LICENSE.TXT')
            Expect    = @(
                'The FreeType Project LICENSE',
                'Redistribution in binary form',
                'FREETYPE LICENSES',
                'zlib license',
                'HarfBuzz library'
            )
            Evidence  = 'M'
            Basis     = 'VersionInfo FileVersion reads 2.14.3, matching the freetype tree gvsbuild built from; share\doc\freetype exists in the prefix and is EMPTY (0 files), so both texts are taken from that source tree rather than the install. LICENSE.TXT is not a duplicate of FTL.TXT: it discloses the zlib, X11-style (BDF/PCF), HarfBuzz Old-MIT and public-domain code compiled into the same DLL, which nothing else in the prefix declares. FTL section 2 is NOT discharged by either file -- see PROVENANCE.md.'
        },
        @{
            Id        = 'fontconfig'
            Component = 'fontconfig'
            Match     = '^bin\\fontconfig-1\.dll$'
            License   = 'MIT'
            Source    = 'prefix:share\doc\fontconfig\COPYING'
            Expect    = 'Permission to use, copy, modify'
            Evidence  = 'I'
            Basis     = 'Mapped by the fontconfig pkg-config module; the COPYING names fontconfig in its first line.'
        },
        @{
            Id        = 'harfbuzz'
            Component = 'HarfBuzz'
            Match     = '^bin\\harfbuzz(-subset)?\.dll$'
            License   = 'MIT'
            Source    = 'prefix:share\doc\harfbuzz\COPYING'
            Expect    = 'Permission is hereby granted'
            Evidence  = 'I'
            Basis     = 'Mapped by the harfbuzz and harfbuzz-subset pkg-config modules; the COPYING names HarfBuzz in its first line.'
        },
        @{
            Id        = 'fribidi'
            Component = 'GNU FriBidi'
            Match     = '^bin\\fribidi-0\.dll$'
            License   = 'LGPL-2.1-or-later'
            Source    = 'prefix:share\doc\fribidi\COPYING'
            Expect    = 'GNU LESSER GENERAL PUBLIC LICENSE'
            Evidence  = 'M'
            Basis     = 'The DLL''s own bytes contain the string "GNU Lesser"; corroborated by the fribidi pkg-config module.'
        },
        @{
            Id        = 'pixman'
            Component = 'pixman'
            Match     = '^bin\\pixman-1-0\.dll$'
            License   = 'MIT'
            Source    = 'prefix:share\doc\pixman\COPYING'
            Expect    = 'Permission is hereby granted'
            Evidence  = 'I'
            Basis     = 'Mapped by the pixman-1 pkg-config module.'
        },
        @{
            Id        = 'libepoxy'
            Component = 'libepoxy'
            Match     = '^bin\\epoxy-0\.dll$'
            License   = 'MIT'
            Source    = 'prefix:share\doc\libepoxy\COPYING'
            Expect    = 'Permission is hereby granted'
            Evidence  = 'I'
            Basis     = 'Mapped by the epoxy pkg-config module; the COPYING names libepoxy in its first line.'
        },

        # -- codecs, parsers and the general-purpose libraries ---------------

        @{
            Id        = 'libpng'
            Component = 'libpng'
            Match     = '^bin\\libpng16\.dll$'
            License   = 'libpng-2.0'
            Source    = 'prefix:share\doc\libpng\LICENSE'
            Expect    = 'PNG Reference Library'
            Evidence  = 'M'
            Basis     = 'The DLL''s own bytes contain the string "libpng"; corroborated by the libpng16 pkg-config module.'
        },
        @{
            Id        = 'libjpeg-turbo'
            Component = 'libjpeg-turbo'
            Match     = '^bin\\jpeg62\.dll$'
            License   = 'IJG AND BSD-3-Clause AND Zlib'
            Source    = 'prefix:share\doc\libjpeg-turbo\LICENSE.md'
            Expect    = 'Redistribution and use in source and binary forms'
            Evidence  = 'M'
            Basis     = 'VersionInfo ProductName reads "libjpeg-turbo".'
        },
        @{
            Id        = 'libtiff'
            Component = 'LibTIFF'
            Match     = '^bin\\tiff\.dll$'
            License   = 'libtiff'
            Source    = 'repo:packaging\windows\licenses\libtiff\LICENSE.md'
            Expect    = 'Permission to use, copy, modify'
            Evidence  = 'M'
            Basis     = 'VersionInfo ProductName reads "LibTIFF" and its LegalCopyright reads "See LICENCE.md" -- a file the prefix does not contain, under 366 files of Sphinx manual.'
        },
        @{
            Id        = 'libxml2'
            Component = 'libxml2'
            Match     = '^bin\\xml2-16\.dll$'
            License   = 'MIT'
            Source    = 'repo:packaging\windows\licenses\libxml2\Copyright'
            Expect    = 'Permission is hereby granted'
            Evidence  = 'I'
            Basis     = 'Mapped by the libxml-2.0 pkg-config module; share\doc\libxml2 exists in the prefix and is EMPTY (0 files).'
        },
        @{
            Id        = 'expat'
            Component = 'Expat'
            Match     = '^bin\\libexpat\.dll$'
            License   = 'MIT'
            Source    = 'prefix:share\doc\expat\COPYING'
            Expect    = 'Permission is hereby granted'
            Evidence  = 'M'
            Basis     = 'The DLL''s own bytes contain the string "Expat".'
        },
        @{
            # VENDORED, not taken from the prefix. share\doc\pcre2\COPYING is four
            # lines saying to read a LICENCE file gvsbuild does not install, and
            # upstream has since renamed that file to LICENCE.md. The 97-byte
            # pointer is what upstream ships under the name COPYING, so this is not
            # gvsbuild dropping anything.
            Id        = 'pcre2'
            Component = 'PCRE2'
            Match     = '^bin\\pcre2-8-0\.dll$'
            License   = 'BSD-3-Clause'
            Source    = 'repo:packaging\windows\licenses\pcre2\LICENCE.md'
            Expect    = 'Redistribution and use in source and binary forms'
            Evidence  = 'M'
            Basis     = 'The DLL''s own bytes contain the string "PCRE2"; vendored at tag pcre2-10.47, the version the prefix reports.'
        },
        @{
            Id        = 'zlib'
            Component = 'zlib'
            Match     = '^bin\\zlib1\.dll$'
            License   = 'Zlib'
            Source    = 'prefix:share\doc\zlib\README'
            Expect    = 'This software is provided'
            Evidence  = 'M'
            Basis     = 'VersionInfo ProductName reads "zlib". The licence is a paragraph at the end of the README; there is no separate file.'
        },
        @{
            Id        = 'libffi'
            Component = 'libffi'
            Match     = '^bin\\ffi-8\.dll$'
            License   = 'MIT'
            Source    = 'prefix:share\doc\libffi\LICENSE'
            Expect    = 'Permission is hereby granted'
            Evidence  = 'I'
            Basis     = 'Mapped by the libffi pkg-config module; the LICENSE names libffi in its first line.'
        },
        @{
            Id        = 'win-iconv'
            Component = 'win-iconv'
            Match     = '^bin\\iconv\.dll$'
            License   = 'Unlicense'
            Source    = 'prefix:share\doc\win-iconv\COPYING'
            Expect    = 'public domain'
            Evidence  = 'I'
            Basis     = 'Mapped by name to the win-iconv doc directory, the only iconv implementation in the prefix.'
        },
        @{
            # THE LICENCE IN THE PREFIX IS THE WRONG ONE. We ship libintl, which is
            # LGPL-2.1-or-later. gvsbuild's share\doc\gettext\COPYING is the GPL-3.0
            # that covers the gettext TOOLS. Staging it would attach a GPL-3 notice
            # to a component that is not under it, so the text is vendored instead.
            Id        = 'gettext-runtime'
            Component = 'GNU gettext (libintl runtime)'
            Match     = '^bin\\intl\.dll$'
            License   = 'LGPL-2.1-or-later'
            Source    = 'repo:packaging\windows\licenses\gettext\COPYING.LIB'
            Expect    = 'GNU LESSER GENERAL PUBLIC LICENSE'
            Evidence  = 'M'
            Basis     = 'VersionInfo ProductName reads "GNU libintl: accessing NLS message catalogs"; the prefix''s gettext COPYING is GPL-3.0, measured from its first two lines.'
        },

        # -- the Microsoft runtime -------------------------------------------

        @{
            # No text ships with the redist directory; the terms are in the Visual
            # Studio licence, which permits app-local distribution of these files.
            # Vendored so that what we relied on is written down at the version we
            # relied on it.
            Id        = 'msvc-runtime'
            Component = 'Microsoft Visual C++ Runtime (app-local redistributable)'
            Match     = '^bin\\vcruntime140(_1)?\.dll$'
            License   = 'LicenseRef-Microsoft-Visual-Studio-Redistributable'
            Source    = 'repo:packaging\windows\licenses\msvc-runtime\REDIST-TERMS.txt'
            Expect    = 'Distributable Code'
            Evidence  = 'M'
            Basis     = 'VersionInfo ProductName reads "Microsoft(R) Visual Studio(R)"; staged from the VC\Redist directory, not System32.'
        }
    )
}
