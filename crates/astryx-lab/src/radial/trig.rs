//! `Math.sin` and `Math.cos` as V8 computes them: fdlibm's kernels and argument
//! reduction, so a chart's coordinates match upstream's to the last digit (a platform
//! libm can differ in the last place).
//!
//! Adapted from fdlibm (`k_sin.c`, `k_cos.c`, `e_rem_pio2.c`, `s_sin.c`,
//! `s_cos.c`), as V8's `src/base/ieee754.cc` carries it:
//!
//! > Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//! > Developed at SunSoft, a Sun Microsystems, Inc. business.
//! > Permission to use, copy, modify, and distribute this software is freely granted,
//! > provided that this notice is preserved.
//!
//! Arguments beyond 2¹⁹·π/2 (no chart angle comes near) fall back to the platform's.

// fdlibm's constants are kept digit for digit, as its source writes them.
#![allow(clippy::excessive_precision, clippy::approx_constant)]

fn high(x: f64) -> i32 {
    (x.to_bits() >> 32) as i32
}

fn with_high(h: i32, low: u32) -> f64 {
    f64::from_bits(((h as u32 as u64) << 32) | low as u64)
}

fn kernel_cos(x: f64, y: f64) -> f64 {
    const C1: f64 = 4.16666666666666019037e-02;
    const C2: f64 = -1.38888888888741095749e-03;
    const C3: f64 = 2.48015872894767294178e-05;
    const C4: f64 = -2.75573143513906633035e-07;
    const C5: f64 = 2.08757232129817482790e-09;
    const C6: f64 = -1.13596475577881948265e-11;
    let ix = high(x) & 0x7fff_ffff;
    if ix < 0x3e40_0000 && x as i32 == 0 {
        return 1.;
    }
    let z = x * x;
    let r = z * (C1 + z * (C2 + z * (C3 + z * (C4 + z * (C5 + z * C6)))));
    if ix < 0x3fd3_3333 {
        return 1. - (0.5 * z - (z * r - x * y));
    }
    let qx = if ix > 0x3fe9_0000 {
        0.28125
    } else {
        with_high(ix - 0x0020_0000, 0)
    };
    let hz = 0.5 * z - qx;
    let a = 1. - qx;
    a - (hz - (z * r - x * y))
}

fn kernel_sin(x: f64, y: f64, iy: i32) -> f64 {
    const S1: f64 = -1.66666666666666324348e-01;
    const S2: f64 = 8.33333333332248946124e-03;
    const S3: f64 = -1.98412698298579493134e-04;
    const S4: f64 = 2.75573137070700676789e-06;
    const S5: f64 = -2.50507602534068634195e-08;
    const S6: f64 = 1.58969099521155010221e-10;
    let ix = high(x) & 0x7fff_ffff;
    if ix < 0x3e40_0000 && x as i32 == 0 {
        return x;
    }
    let z = x * x;
    let v = z * x;
    let r = S2 + z * (S3 + z * (S4 + z * (S5 + z * S6)));
    if iy == 0 {
        x + v * (S1 + z * r)
    } else {
        x - ((z * (0.5 * y - v * r) - y) - v * S1)
    }
}

const NPIO2_HW: [i32; 32] = [
    0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C, 0x4025FDBB, 0x402921FB,
    0x402C463A, 0x402F6A7A, 0x4031475C, 0x4032D97C, 0x40346B9C, 0x4035FDBB, 0x40378FDB, 0x403921FB,
    0x403AB41B, 0x403C463A, 0x403DD85A, 0x403F6A7A, 0x40407E4C, 0x4041475C, 0x4042106C, 0x4042D97C,
    0x4043A28C, 0x40446B9C, 0x404534AC, 0x4045FDBB, 0x4046C6CB, 0x40478FDB, 0x404858EB, 0x404921FB,
];

/// `x` less a multiple `n` of π/2, as a head and a tail; `None` past the medium range.
fn rem_pio2(x: f64) -> Option<(i32, f64, f64)> {
    const INVPIO2: f64 = 6.36619772367581382433e-01;
    const PIO2_1: f64 = 1.57079632673412561417e+00;
    const PIO2_1T: f64 = 6.07710050650619224932e-11;
    const PIO2_2: f64 = 6.07710050630396597660e-11;
    const PIO2_2T: f64 = 2.02226624879595063154e-21;
    const PIO2_3: f64 = 2.02226624871116645580e-21;
    const PIO2_3T: f64 = 8.47842766036889956997e-32;
    let hx = high(x);
    let ix = hx & 0x7fff_ffff;
    if ix <= 0x3fe9_21fb {
        return Some((0, x, 0.));
    }
    if ix < 0x4002_d97c {
        // |x| < 3π/4: n is ±1.
        return Some(if hx > 0 {
            let mut z = x - PIO2_1;
            let (y0, y1) = if ix != 0x3ff9_21fb {
                let y0 = z - PIO2_1T;
                (y0, (z - y0) - PIO2_1T)
            } else {
                z -= PIO2_2;
                let y0 = z - PIO2_2T;
                (y0, (z - y0) - PIO2_2T)
            };
            (1, y0, y1)
        } else {
            let mut z = x + PIO2_1;
            let (y0, y1) = if ix != 0x3ff9_21fb {
                let y0 = z + PIO2_1T;
                (y0, (z - y0) + PIO2_1T)
            } else {
                z += PIO2_2;
                let y0 = z + PIO2_2T;
                (y0, (z - y0) + PIO2_2T)
            };
            (-1, y0, y1)
        });
    }
    if ix > 0x4139_21fb {
        return None;
    }
    let mut t = x.abs();
    let n = (t * INVPIO2 + 0.5) as i32;
    let f = f64::from(n);
    let mut r = t - f * PIO2_1;
    let mut w = f * PIO2_1T;
    let mut y0;
    if n < 32 && ix != NPIO2_HW[(n - 1) as usize] {
        y0 = r - w;
    } else {
        let j = ix >> 20;
        y0 = r - w;
        let mut i = j - ((high(y0) >> 20) & 0x7ff);
        if i > 16 {
            t = r;
            w = f * PIO2_2;
            r = t - w;
            w = f * PIO2_2T - ((t - r) - w);
            y0 = r - w;
            i = j - ((high(y0) >> 20) & 0x7ff);
            if i > 49 {
                t = r;
                w = f * PIO2_3;
                r = t - w;
                w = f * PIO2_3T - ((t - r) - w);
                y0 = r - w;
            }
        }
    }
    let y1 = (r - y0) - w;
    Some(if hx < 0 { (-n, -y0, -y1) } else { (n, y0, y1) })
}

/// `Math.cos`.
pub fn js_cos(x: f64) -> f64 {
    let ix = high(x) & 0x7fff_ffff;
    if ix <= 0x3fe9_21fb {
        return kernel_cos(x, 0.);
    }
    if ix >= 0x7ff0_0000 {
        // fdlibm's `x - x`: NaN for an infinity or a NaN.
        return f64::NAN;
    }
    let Some((n, a, b)) = rem_pio2(x) else {
        return x.cos();
    };
    match n & 3 {
        0 => kernel_cos(a, b),
        1 => -kernel_sin(a, b, 1),
        2 => -kernel_cos(a, b),
        _ => kernel_sin(a, b, 1),
    }
}

/// `Math.sin`.
pub fn js_sin(x: f64) -> f64 {
    let ix = high(x) & 0x7fff_ffff;
    if ix <= 0x3fe9_21fb {
        return kernel_sin(x, 0., 0);
    }
    if ix >= 0x7ff0_0000 {
        // fdlibm's `x - x`: NaN for an infinity or a NaN.
        return f64::NAN;
    }
    let Some((n, a, b)) = rem_pio2(x) else {
        return x.sin();
    };
    match n & 3 {
        0 => kernel_sin(a, b, 1),
        1 => kernel_cos(a, b),
        2 => -kernel_sin(a, b, 1),
        _ => -kernel_cos(a, b),
    }
}
