# HTML Injection

A script tag (must not execute):

<script>console.log('SCRIB-XSS-EXECUTED');alert('xss')</script>

An iframe reading outside the folder (must not load):

<iframe src="file:///etc/passwd"></iframe>

An onerror handler (must not fire):

<img src="does-not-exist.png" onerror="console.log('SCRIB-ONERROR-FIRED')">

Normal paragraph after the injection attempts.
