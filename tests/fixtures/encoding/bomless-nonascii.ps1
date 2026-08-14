# Fixture for check 12. NO BOM + non-ASCII in a string literal = the fatal case.
# Windows PowerShell 5.1 decodes this file as the ANSI codepage, the literal ends
# early at the mis-decoded byte, and the parse dies. Nothing runs this file; it exists
# so check 12 has something it must refuse.
$msg = "an em dash — inside a literal"
