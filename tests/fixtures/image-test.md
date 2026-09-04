# Image test

Same-dir relative image (220px — must NOT stretch to fill the pane):

![logo](logo.png)

Wide image (1600px — must scale down to FIT the pane, never blank; TDD 2.21):

![wide](wide.png)

Vector image (240px SVG with text in it — must scale with zoom AND stay sharp; TDD 13.11):

![diagram](diagram.svg)

Traversal (must be refused):

![evil](../../../etc/hosts)
