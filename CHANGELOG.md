# Changelog

## Unreleased

### Reader

- Account for valid UTF-8 copies at their actual byte length while retaining a
  conservative allowance for lossy decoding; avoid oversized initial allocation
  for large vector elements. Public malformed-input and allocation-boundary tests
  cover these changes. Default parser limits are unchanged.
- Publish separate reader/consumer capability states, evidence requirements and
  remaining limitations in [CAPABILITIES.md](CAPABILITIES.md).

### Experimental EffectCraft consumer

- Import bounded static stars/polygons, gradient snapshots and repeaters; add
  round corners, pucker/bloat, twist, zigzag, merge and single-copy offset paths.
  Selected operator parameters retain numeric keys. Unsupported geometry and
  excessive expansions produce local omissions with warnings.
- Map saved stroke/fill/trim enums, separated position, time remapping, marker
  points, camera/light fields, material switches, project bit depth and audio rate.
  Unverified marker duration, color pipelines and material units stay explicit.
- Extend text animation beyond tracking and character offset to common planar
  transform, opacity and fill/stroke channels. Preserve ordered range selectors,
  percent/index bounds and known Advanced fields. Malformed or unsupported
  animators are omitted transactionally without discarding the text layer or
  reporting rejected keys as imported.
- Allow opt-in recovery of missing parent references. The desktop converter uses
  this mode and reports loss of world-space placement; the library default still
  rejects invalid parents. Cycles remain errors.
- Give the desktop adapter a separate 768 MiB cumulative parser-allocation budget,
  one-million-chunk limit and 256 MiB input limit for larger projects.

These additions do not establish complete AE rendering parity. SourceText
content animation, many effects/operators, media relinking and color management
remain unsupported or unverified. See [validation](VALIDATION.md) and the
[adapter README](examples/effectcraft/README.md).
