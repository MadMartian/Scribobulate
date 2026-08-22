/*
 * allowed-file-types-empty.m -- does -[NSSavePanel setAllowedFileTypes:] actually
 * raise on a non-nil EMPTY array?
 *
 * Apple's SDK header says it does: "If the array is not nil and the array contains
 * no items, an exception will be raised." A crash report against GTK was about to
 * be filed on the strength of that sentence plus a source chain showing GTK can
 * reach the call with an empty array.
 *
 * On macOS 26.6.1 it raises nothing -- not for @[], not for nil, not for @[@""] --
 * and the empty array reads back intact. The documented contract is not the
 * shipping behaviour. A plausible reading, INFERRED and untested: the setter is
 * deprecated since macOS 12 and is now bridged onto allowedContentTypes, where @[]
 * is the meaningful "no restriction" value rather than an invalid one; that would
 * put the relaxation somewhere around the bridging work, and the original contract
 * may still hold on macOS 11 and earlier.
 *
 * The lesson this file exists to carry: an SDK header is primary evidence for API
 * SURFACE -- that a class is not a singleton, that a property is `copy`, that a
 * symbol is deprecated. It is NOT evidence of RUNTIME BEHAVIOUR. Exceptions
 * raised, objects freed, handlers invoked: those need an observation. And a
 * deprecated symbol's documented contract should be assumed stale until someone
 * watches it, because deprecation is exactly when an implementation gets rewritten
 * underneath its documentation.
 *
 * Build:
 *   clang -ObjC -O1 -o /tmp/allowed-file-types-empty \
 *       probes/allowed-file-types-empty.m -framework AppKit
 */
#import <AppKit/AppKit.h>
#include <stdio.h>

#define TRY(label, expr)                                                        \
    @try {                                                                      \
        expr;                                                                   \
        printf("  %-26s NO exception\n", label);                                \
    } @catch (NSException * e) {                                                \
        printf("  %-26s RAISED %s: %s\n", label, [[e name] UTF8String],         \
               [[e reason] UTF8String]);                                        \
    }

int main(void)
{
    @autoreleasepool {
        [NSApplication sharedApplication];
        NSSavePanel *p = [[NSSavePanel savePanel] retain];

        printf("-[NSSavePanel setAllowedFileTypes:] / setAllowedContentTypes:\n");

        /* POSITIVE CONTROL, and it runs FIRST because everything below depends on it.
         * The four cases each print "NO exception", which is only evidence if this rig
         * can print anything else -- a TRY macro whose @catch is unreachable, an
         * exception model that is off, or a build where these calls cannot raise at all
         * would produce the identical four lines and read as a clean result. Indexing an
         * empty NSArray raises NSRangeException unconditionally, so if THIS prints "NO
         * exception" the instrument is broken and no verdict below is worth reading. */
        int control_raised = 0;
        @try {
            (void)[[NSArray array] objectAtIndex:0];
            printf("  %-26s NO exception\n", "control (must RAISE)");
        } @catch (NSException * e) {
            control_raised = 1;
            printf("  %-26s RAISED %s (control OK)\n", "control (must RAISE)",
                   [[e name] UTF8String]);
        }
        if (!control_raised) {
            printf("INVALID: the positive control did not raise, so every \"NO exception\"\n"
                   "         below is unproven -- this rig cannot report a raise at all.\n");
            return 2;
        }

        TRY("empty array @[]", [p setAllowedFileTypes:@[]]);
        printf("  %-26s reads back as %s\n", "  after @[]",
               [p allowedFileTypes] ? [[[p allowedFileTypes] description] UTF8String]
                                    : "(nil)");
        TRY("nil", [p setAllowedFileTypes:nil]);
        TRY("array containing @\"\"", [p setAllowedFileTypes:@[ @"" ]]);
        TRY("allowedContentTypes @[]", [p setAllowedContentTypes:@[]]);
    }
    return 0;
}
