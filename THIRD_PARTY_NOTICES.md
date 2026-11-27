# Third-party notices

## Astryx

The design, tokens, themes, behavior specifications, and component structure
come from [Astryx](https://github.com/meta/astryx), MIT License,
Copyright (c) 2026 Meta Platforms, Inc. See [`LICENSE`](LICENSE). Core's
fallback icons (`crates/astryx-icons/assets/core/`) are Astryx's own.

## Lucide icons

`crates/astryx-icons/assets/lucide/` holds the Lucide icons the published
themes map semantic names to, generated from lucide-react's icon data.
ISC License, Copyright (c) 2026 Lucide Icons and Contributors; the license text
is in that folder.

## Fonts

Theme fonts are unmodified static TTF files as served by Google Fonts. Each
family is licensed under the SIL Open Font License 1.1; its license text
(`OFL.txt`) sits next to its files in `crates/astryx-themes/fonts/<family>/`.

| Family | Used by | Reserved Font Name |
|---|---|---|
| Albert Sans | chocolate | — |
| Crimson Text | y2k | — |
| DM Sans | matcha | — |
| Figtree | neutral, stone; bundled fallback | — |
| Fraunces | chocolate | — |
| Fustat | gothic | — |
| JetBrains Mono | most themes' code font; bundled fallback | — |
| Manufacturing Consent | gothic | — |
| Montserrat | stone | — |
| Outfit | butter | — |
| Playwrite US Trad | matcha | — |
| Poppins | y2k | — |
| Sarina | butter | "Sarina" |
| UnifrakturMaguntia | gothic | "UnifrakturMaguntia" |

The files are redistributed unmodified, as the OFL permits; the Reserved Font
Names restrict only modified versions.

## V8

`crates/astryx-lab/src/three_d/sort.rs` follows V8's TimSort
(`third_party/v8/builtins/array-sort.tq`) so that the 3D charts' depth order
matches `Array.prototype.sort`; `crates/astryx-lab/src/trig.rs` follows V8's
`src/base/ieee754.cc` (see fdlibm below). BSD-3-Clause License,
Copyright 2018 the V8 project authors. All rights reserved.

    Redistribution and use in source and binary forms, with or without
    modification, are permitted provided that the following conditions are
    met:

        * Redistributions of source code must retain the above copyright
          notice, this list of conditions and the following disclaimer.
        * Redistributions in binary form must reproduce the above
          copyright notice, this list of conditions and the following
          disclaimer in the documentation and/or other materials provided
          with the distribution.
        * Neither the name of Google Inc. nor the names of its
          contributors may be used to endorse or promote products derived
          from this software without specific prior written permission.

    THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
    "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
    LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
    A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
    OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
    SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
    LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
    DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
    THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
    (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
    OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## fdlibm

`crates/astryx-lab/src/trig.rs` adapts fdlibm's sine, cosine and argument
reduction (`k_sin.c`, `k_cos.c`, `e_rem_pio2.c`, `k_rem_pio2.c`, `s_sin.c`,
`s_cos.c`) as V8 carries them, so that `Math.sin` and `Math.cos` match V8's
to the bit; the radial and 3D charts use it.

    Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.

    Developed at SunSoft, a Sun Microsystems, Inc. business.
    Permission to use, copy, modify, and distribute this
    software is freely granted, provided that this notice
    is preserved.
