# Image test

Same-dir relative image (220px — must NOT stretch to fill the pane):

![logo](logo.png)

Wide image (1600px — must scale down to FIT the pane, never blank; TDD 2.21):

![wide](wide.png)

Vector image (240px SVG with text in it — must scale with zoom AND stay sharp; TDD 13.11):

![diagram](diagram.svg)

Traversal (must be refused):

![evil](../../../etc/hosts)

Alt text made of MARKUP (TDD 2.5) — nothing below the picture, and no second image:

![a `--flag` span, a [link](https://example.invalid/), a nested ![inner](logo.png) and a raw <img src="logo.png"> tail](logo.png)

Declared size (TDD 2.21b) — `width="100"` (logo at 100 px wide), `height="40px"` (logo 40 px tall, aspect kept), `width="5000"` (wide.png, clamped to the pane), `width="50%"` (ignored: natural size):

<img src="logo.png" width="100">

<img src="logo.png" height="40px">

<img src="wide.png" width="5000">

<img src="logo.png" width="50%">
