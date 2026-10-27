# Owned current-attribute Mikk structure compiler

`mikktspace::compile_structure(&MeshData)` shares the existing current-attribute preparation and stops immediately after `Build4RuleGroups`. It returns owned encoded welded indices, full prepared triangle order (including inactive tails), pre-group flags, post-group flags, neighbors, active count, assigned group indices and ordered group members/representatives/orientation. It does not allocate tangent output or call the tangent evaluator, epilogue or Geometry setter. There are no stored derivatives, angles or final tangents. The ordinary CPU rendering path continues through its existing branch.

Empty and all-degenerate inputs return their actual structural state. The compiler isolates an existing nested weld-validation context with a disabled sentinel; it does not use test-capture TLS to obtain its output. Owned numeric group indices are copied while the original preparation allocation is alive; no pointer escapes.

Synthetic checks compare against the independent existing capture interface for empty input, all/mixed degenerates, UV-zero Any triangles and same-count topology changes. A Geometry setter that panics proves no tangent emission; ordinary full generation on the same geometry reaches that setter as a positive control. A nested unrelated weld-validation context produces the same owned result.

The actual saved Ray scene passed all317 integer/half-frame poses across13 material parts:4121 owned recipes matched the existing pre/post-group captures, including every field and ordered membership. This initial correctness run overlapped other correctness jobs, so its elapsed time is not a performance measurement. The ordinary Bevy suite passed52 unit tests, one GPU bake test and seven GPU render tests, with23 opt-in tests ignored.

The compiler is not connected to the production renderer. GPU recipe consumption and its numerical/image validation remain separate work.

A separate quiet release run compiled all13 parts at poses0,39.25 and149, repeating each part32 times:32.528ms per complete13-part pose (4.84s total test including scene setup). This is fresh serial preparation, without persistent weld/neighbor reuse or parallel part batching. It is a recipe compilation cost, not a native playback FPS claim.
