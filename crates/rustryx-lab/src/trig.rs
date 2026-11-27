//! `Math.sin` and `Math.cos` as V8 computes them: fdlibm's kernels and argument
//! reduction, so a chart's coordinates match upstream's to the last digit (a platform
//! libm can differ in the last place). Shared by the radial and 3D charts.
//!
//! Adapted from fdlibm (`k_sin.c`, `k_cos.c`, `e_rem_pio2.c`, `k_rem_pio2.c`, `s_sin.c`,
//! `s_cos.c`), as V8's `src/base/ieee754.cc` carries it:
//!
//! > Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//! > Developed at SunSoft, a Sun Microsystems, Inc. business.
//! > Permission to use, copy, modify, and distribute this software is freely granted,
//! > provided that this notice is preserved.

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

/// `x` less a multiple `n` of π/2, as a head and a tail (fdlibm's `__ieee754_rem_pio2`).
fn rem_pio2(x: f64) -> (i32, f64, f64) {
    match rem_pio2_medium(x) {
        Some(reduced) => reduced,
        None => rem_pio2_large(x),
    }
}

/// The reduction up to 2¹⁹·π/2; `None` past it.
fn rem_pio2_medium(x: f64) -> Option<(i32, f64, f64)> {
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

const TWO24: f64 = 16_777_216.;
const TWON24: f64 = 5.960_464_477_539_062_5e-8;

/// fdlibm's `two_over_pi`: 2/π in 24-bit chunks.
const TWO_OVER_PI: [i32; 66] = [
    0xA2F983, 0x6E4E44, 0x1529FC, 0x2757D1, 0xF534DD, 0xC0DB62, 0x95993C, 0x439041, 0xFE5163,
    0xABDEBB, 0xC561B7, 0x246E3A, 0x424DD2, 0xE00649, 0x2EEA09, 0xD1921C, 0xFE1DEB, 0x1CB129,
    0xA73EE8, 0x8235F5, 0x2EBB44, 0x84E99C, 0x7026B4, 0x5F7E41, 0x3991D6, 0x398353, 0x39F49C,
    0x845F8B, 0xBDF928, 0x3B1FF8, 0x97FFDE, 0x05980F, 0xEF2F11, 0x8B5A0A, 0x6D1F6D, 0x367ECF,
    0x27CB09, 0xB74F46, 0x3F669E, 0x5FEA2D, 0x7527BA, 0xC7EBE5, 0xF17B3D, 0x0739F7, 0x8A5292,
    0xEA6BFB, 0x5FB11F, 0x8D5D08, 0x560330, 0x46FC7B, 0x6BABF0, 0xCFBC20, 0x9AF436, 0x1DA9E3,
    0x91615E, 0xE61B08, 0x659985, 0x5F14A0, 0x68408D, 0xFFD880, 0x4D7327, 0x310606, 0x1556CA,
    0x73A8C9, 0x60E27B, 0xC08C6B,
];

/// fdlibm's `PIo2`: π/2 in 24-bit pieces.
const PIO2: [u64; 8] = [
    0x3FF921FB_40000000,
    0x3E74442D_00000000,
    0x3CF84698_80000000,
    0x3B78CC51_60000000,
    0x39F01B83_80000000,
    0x387A2520_40000000,
    0x36E38222_80000000,
    0x3569F31D_00000000,
];

/// `x × 2ⁿ`, exact while the result is normal (C's `scalbn`).
fn scalbn(x: f64, n: i32) -> f64 {
    let mut y = x;
    let mut n = n;
    while n > 1023 {
        y *= f64::from_bits(0x7FE0_0000_0000_0000);
        n -= 1023;
    }
    while n < -1022 {
        // 2⁻¹⁰²² × 2⁵³, keeping precision through the subnormal range.
        y *= f64::from_bits(0x0010_0000_0000_0000) * f64::from_bits(0x4340_0000_0000_0000);
        n += 1022 - 53;
    }
    y * f64::from_bits(((0x3FF + n) as u64) << 52)
}

/// The reduction past 2¹⁹·π/2: `|x|` cut into 24-bit pieces and reduced against as many
/// bits of 2/π as it needs.
fn rem_pio2_large(x: f64) -> (i32, f64, f64) {
    let hx = high(x);
    let ix = hx & 0x7fff_ffff;
    let e0 = (ix >> 20) - 1046;
    let mut z = with_high(ix - (e0 << 20), x.to_bits() as u32);
    let mut tx = [0.; 3];
    for t in tx.iter_mut().take(2) {
        *t = f64::from(z as i32);
        z = (z - *t) * TWO24;
    }
    tx[2] = z;
    let mut nx = 3;
    while tx[nx - 1] == 0. {
        nx -= 1;
    }
    let (n, y0, y1) = kernel_rem_pio2(&tx[..nx], e0);
    if hx < 0 { (-n, -y0, -y1) } else { (n, y0, y1) }
}

/// fdlibm's `__kernel_rem_pio2` at double precision (`prec` 2): `x` (24-bit pieces, the
/// first scaled by `2^e0`) less a multiple of π/2, that multiple's last three bits, and the
/// remainder as a head and a tail.
fn kernel_rem_pio2(x: &[f64], e0: i32) -> (i32, f64, f64) {
    const JK: usize = 4;
    const JP: usize = JK;
    let jx = x.len() - 1;
    let jv = ((e0 - 3) / 24).max(0);
    let mut q0 = e0 - 24 * (jv + 1);
    let jv = jv as usize;
    let (mut f, mut q, mut fq, mut iq) = ([0f64; 20], [0f64; 20], [0f64; 20], [0i32; 20]);
    // f[i] = two_over_pi[jv - jx + i], zero before the table.
    for (i, slot) in f.iter_mut().take(jx + JK + 1).enumerate() {
        let j = (jv + i).checked_sub(jx);
        *slot = j.map_or(0., |j| f64::from(TWO_OVER_PI[j]));
    }
    for i in 0..=JK {
        let mut fw = 0.;
        for (j, xj) in x.iter().enumerate() {
            fw += xj * f[jx + i - j];
        }
        q[i] = fw;
    }
    let mut jz = JK;
    let (mut z, mut n, mut ih);
    loop {
        // Distill q[] into iq[], reversed.
        z = q[jz];
        for (i, j) in (1..=jz).rev().enumerate() {
            let fw = f64::from((TWON24 * z) as i32);
            iq[i] = (z - TWO24 * fw) as i32;
            z = q[j - 1] + fw;
        }
        // n, and the fraction.
        z = scalbn(z, q0);
        z -= 8. * (z * 0.125).floor();
        n = z as i32;
        z -= f64::from(n);
        ih = 0;
        if q0 > 0 {
            let i = iq[jz - 1] >> (24 - q0);
            n += i;
            iq[jz - 1] -= i << (24 - q0);
            ih = iq[jz - 1] >> (23 - q0);
        } else if q0 == 0 {
            ih = iq[jz - 1] >> 23;
        } else if z >= 0.5 {
            ih = 2;
        }
        if ih > 0 {
            // q > 0.5: take 1 - q.
            n += 1;
            let mut carry = 0;
            for digit in iq.iter_mut().take(jz) {
                let j = *digit;
                if carry == 0 {
                    if j != 0 {
                        carry = 1;
                        *digit = 0x100_0000 - j;
                    }
                } else {
                    *digit = 0xff_ffff - j;
                }
            }
            match q0 {
                1 => iq[jz - 1] &= 0x7f_ffff,
                2 => iq[jz - 1] &= 0x3f_ffff,
                _ => {}
            }
            if ih == 2 {
                z = 1. - z;
                if carry != 0 {
                    z -= scalbn(1., q0);
                }
            }
        }
        // Recompute with more bits of 2/π when the fraction came out zero.
        if z == 0. && iq[JK..jz].iter().fold(0, |acc, d| acc | d) == 0 {
            let mut k = 1;
            while iq[JK - k] == 0 {
                k += 1;
            }
            for i in jz + 1..=jz + k {
                f[jx + i] = f64::from(TWO_OVER_PI[jv + i]);
                let mut fw = 0.;
                for (j, xj) in x.iter().enumerate() {
                    fw += xj * f[jx + i - j];
                }
                q[i] = fw;
            }
            jz += k;
            continue;
        }
        break;
    }
    // Chop off zero terms, or split z into 24-bit pieces.
    if z == 0. {
        jz -= 1;
        q0 -= 24;
        while iq[jz] == 0 {
            jz -= 1;
            q0 -= 24;
        }
    } else {
        z = scalbn(z, -q0);
        if z >= TWO24 {
            let fw = f64::from((TWON24 * z) as i32);
            iq[jz] = (z - TWO24 * fw) as i32;
            jz += 1;
            q0 += 24;
            iq[jz] = fw as i32;
        } else {
            iq[jz] = z as i32;
        }
    }
    // The pieces as floating-point values.
    let mut fw = scalbn(1., q0);
    for i in (0..=jz).rev() {
        q[i] = fw * f64::from(iq[i]);
        fw *= TWON24;
    }
    // PIo2[0..=jp] × q[jz..=0].
    for i in (0..=jz).rev() {
        let mut fw = 0.;
        let mut k = 0;
        while k <= JP && k <= jz - i {
            fw += f64::from_bits(PIO2[k]) * q[i + k];
            k += 1;
        }
        fq[jz - i] = fw;
    }
    // Compress fq[] into a head and a tail.
    let mut fw = 0.;
    for i in (0..=jz).rev() {
        fw += fq[i];
    }
    let y0 = if ih == 0 { fw } else { -fw };
    let mut fw = fq[0] - fw;
    for v in &fq[1..=jz] {
        fw += v;
    }
    let y1 = if ih == 0 { fw } else { -fw };
    (n & 7, y0, y1)
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
    let (n, a, b) = rem_pio2(x);
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
    let (n, a, b) = rem_pio2(x);
    match n & 3 {
        0 => kernel_sin(a, b, 1),
        1 => kernel_cos(a, b),
        2 => -kernel_sin(a, b, 1),
        _ => -kernel_cos(a, b),
    }
}

#[cfg(test)]
mod tests {
    use super::{js_cos, js_sin};

    /// (x, Node's `Math.cos(x)`, Node's `Math.sin(x)`) as bits, where the platform's
    /// `cos`/`sin` differ: medium arguments, and large ones past 2¹⁹·π/2.
    const NODE: [(u64, u64, u64); 9] = [
        (0xc004000000000000, 0xbfe9a2f7ef858b7d, 0xbfe326af0dcfcab0),
        (0x412e848000000000, 0x3fedf9df9906d32c, 0xbfd6664b2568d867),
        (0x4133b4fb59b00000, 0x3fee4d354c49305b, 0x3fd492b8550aecff),
        (0x4135b33d31c08000, 0x3fd41a82aaf8f4d9, 0xbfee615a665b6e4f),
        (0x4480f0cf064dd592, 0x3fe0be2cef01c8f4, 0xbfeb453ab76bf397),
        (0x4cf11a44a0000000, 0xbfd4780e70d9406a, 0xbfee51b8a17de752),
        (0x7a95ec38a0000000, 0x3fe61d8eb4450198, 0xbfe720c79589cfc0),
        (0xffe7285ac0000000, 0x3fe561dc85d0752a, 0xbfe7ceba4799e2d1),
        (0xc32d8fcfb0000000, 0xbfee0280d01710d8, 0xbfd637e28fd0518a),
    ];

    /// Degrees turned to radians as the charts do, with Node's results.
    const NODE_DEGREES: [(u64, u64, u64); 3] = [
        (0xc08f400000000000, 0x3fc63a1a7e0b732b, 0x3fef838b8c811c1c),
        (0xc08d980000000000, 0xbfe5d2ee398c9c38, 0x3fe7673fe0c86976),
        (0xc088680000000000, 0x3fdf071eedefa0e1, 0xbfebfce277d339ca),
    ];

    #[test]
    fn matches_v8_to_the_bit() {
        for (x, cos, sin) in NODE {
            let x = f64::from_bits(x);
            assert_eq!(js_cos(x).to_bits(), cos, "cos({x:e})");
            assert_eq!(js_sin(x).to_bits(), sin, "sin({x:e})");
        }
        for (d, cos, sin) in NODE_DEGREES {
            let r = (f64::from_bits(d) * std::f64::consts::PI) / 180.;
            assert_eq!(js_cos(r).to_bits(), cos);
            assert_eq!(js_sin(r).to_bits(), sin);
        }
        assert!(js_cos(f64::INFINITY).is_nan() && js_sin(f64::NAN).is_nan());
        assert_eq!(js_sin(-0f64).to_bits(), (-0f64).to_bits());
    }
}
