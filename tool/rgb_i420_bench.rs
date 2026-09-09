use std::hint::black_box;
use std::time::Instant;

fn baseline(rgb: &[u8], stride: usize, width: usize, height: usize) -> Vec<u8> {
    let y_size = width * height;
    let uv_stride = width / 2;
    let mut output = vec![0u8; y_size + y_size / 2];
    let (y_plane, uv) = output.split_at_mut(y_size);
    let (u_plane, v_plane) = uv.split_at_mut(y_size / 4);
    for row in 0..height {
        for col in 0..width {
            let offset = (row * stride + col) * 3;
            let r = rgb[offset] as i32;
            let g = rgb[offset + 1] as i32;
            let b = rgb[offset + 2] as i32;
            y_plane[row * width + col] = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(0, 255) as u8;
        }
    }
    for row in (0..height).step_by(2) {
        for col in (0..width).step_by(2) {
            let mut r = 0i32;
            let mut g = 0i32;
            let mut b = 0i32;
            for y in 0..2 {
                for x in 0..2 {
                    let offset = ((row + y) * stride + col + x) * 3;
                    r += rgb[offset] as i32;
                    g += rgb[offset + 1] as i32;
                    b += rgb[offset + 2] as i32;
                }
            }
            r /= 4;
            g /= 4;
            b /= 4;
            let index = (row / 2) * uv_stride + col / 2;
            u_plane[index] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
            v_plane[index] = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
        }
    }
    output
}

fn combined(rgb: &[u8], stride: usize, width: usize, height: usize) -> Vec<u8> {
    let y_size = width * height;
    let uv_stride = width / 2;
    let mut output = vec![0u8; y_size + y_size / 2];
    let (y_plane, uv) = output.split_at_mut(y_size);
    let (u_plane, v_plane) = uv.split_at_mut(y_size / 4);
    for row in (0..height).step_by(2) {
        for col in (0..width).step_by(2) {
            let mut r_sum = 0i32;
            let mut g_sum = 0i32;
            let mut b_sum = 0i32;
            for y in 0..2 {
                for x in 0..2 {
                    let pixel_row = row + y;
                    let pixel_col = col + x;
                    let offset = (pixel_row * stride + pixel_col) * 3;
                    let r = rgb[offset] as i32;
                    let g = rgb[offset + 1] as i32;
                    let b = rgb[offset + 2] as i32;
                    y_plane[pixel_row * width + pixel_col] = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(0, 255) as u8;
                    r_sum += r;
                    g_sum += g;
                    b_sum += b;
                }
            }
            let r = r_sum / 4;
            let g = g_sum / 4;
            let b = b_sum / 4;
            let index = (row / 2) * uv_stride + col / 2;
            u_plane[index] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
            v_plane[index] = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
        }
    }
    output
}

fn main() {
    const W: usize = 1920;
    const H: usize = 1080;
    let rgb = (0..W * H * 3).map(|i| ((i * 31 + 17) & 255) as u8).collect::<Vec<_>>();
    assert_eq!(baseline(&rgb, W, W, H), combined(&rgb, W, W, H));
    for _ in 0..8 {
        black_box(baseline(&rgb, W, W, H));
        black_box(combined(&rgb, W, W, H));
    }
    let mut old = Vec::new();
    let mut new = Vec::new();
    for _ in 0..31 {
        let start = Instant::now();
        black_box(baseline(&rgb, W, W, H));
        old.push(start.elapsed().as_micros());
        let start = Instant::now();
        black_box(combined(&rgb, W, W, H));
        new.push(start.elapsed().as_micros());
    }
    old.sort_unstable();
    new.sort_unstable();
    let p50 = |v: &[u128]| v[v.len() / 2];
    let p95 = |v: &[u128]| v[v.len() * 95 / 100];
    println!("baseline p50={}us p95={}us", p50(&old), p95(&old));
    println!("combined p50={}us p95={}us", p50(&new), p95(&new));
    println!("p50 gain={:.1}%", (p50(&old) as f64 - p50(&new) as f64) * 100.0 / p50(&old) as f64);
}
