# ServerQ catalog

`rules.json` contains the exact producer bytes from taskqorg/serverq commit
`6b333b435e6cfcdd81c0c793cf0071da8abbfb5f`. DashQ compares their SHA-256 with
the projection's `catalog` before exposing thresholds or recommendations.
The Linux and Windows projection examples contain invented identities and catalog hashes;
they deliberately do not match this catalog. They are data fixtures, not host tests.
