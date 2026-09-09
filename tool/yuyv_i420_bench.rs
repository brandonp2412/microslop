use std::hint::black_box;
use std::time::Instant;

fn baseline(yuyv: &[u8], width: usize, height: usize) -> Vec<u8> {
    let y_size = width * height;
    let uv_size = (width / 2) * (height / 2);
    let mut out = vec![0u8; y_size + uv_size * 2];
    let (y_plane, uv_planes) = out.split_at_mut(y_size);
    let (u_plane, v_plane) = uv_planes.split_at_mut(uv_size);
    for row in 0..height {
        for col in (0..width).step_by(2) {
            let offset = (row * width + col) * 2;
            if offset + 3 >= yuyv.len() {
                break;
            }
            y_plane[row * width + col] = yuyv[offset];
            y_plane[row * width + col + 1] = yuyv[offset + 2];
            if row % 2 == 0 {
                let uv = (row / 2) * (width / 2) + col / 2;
                u_plane[uv] = yuyv[offset + 1];
                v_plane[uv] = yuyv[offset + 3];
            }
        }
    }
    out
}

fn chunked(yuyv: &[u8], width: usize, height: usize) -> Vec<u8> {
    let y_size = width * height;
    let uv_size = (width / 2) * (height / 2);
    let mut out = vec![0u8; y_size + uv_size * 2];
    let (y_plane, uv_planes) = out.split_at_mut(y_size);
    let (u_plane, v_plane) = uv_planes.split_at_mut(uv_size);
    let row_bytes = width * 2;
    for (row, source) in yuyv.chunks(row_bytes).take(height).enumerate() {
        let target = &mut y_plane[row * width..(row + 1) * width];
        let uv_start = (row / 2) * (width / 2);
        for (pair, pixel) in source.chunks_exact(4).enumerate() {
            let col = pair * 2;
            target[col] = pixel[0];
            target[col + 1] = pixel[2];
            if row % 2 == 0 {
                u_plane[uv_start + pair] = pixel[1];
                v_plane[uv_start + pair] = pixel[3];
            }
        }
    }
    out
}

fn main() {
    const W: usize = 1920;
    const H: usize = 1080;
    let source = (0..W * H * 2).map(|i| ((i * 17 + 3) & 255) as u8).collect::<Vec<_>>();
    assert_eq!(baseline(&source, W, H), chunked(&source, W, H));
    for _ in 0..10 {
        black_box(baseline(&source, W, H));
        black_box(chunked(&source, W, H));
    }
    let mut old = Vec::new();
    let mut new = Vec::new();
    for _ in 0..51 {
        let start = Instant::now();
        black_box(baseline(&source, W, H));
        old.push(start.elapsed().as_micros());
        let start = Instant::now();
        black_box(chunked(&source, W, H));
        new.push(start.elapsed().as_micros());
    }
    old.sort_unstable();
    new.sort_unstable();
    let p50 = |v: &[u128]| v[v.len() / 2];
    let p95 = |v: &[u128]| v[v.len() * 95 / 100];
    println!("baseline p50={}us p95={}us", p50(&old), p95(&old));
    println!("chunked  p50={}us p95={}us", p50(&new), p95(&new));
    println!("p50 gain={:.1}%", (p50(&old) as f64 - p50(&new) as f64) * 100.0 / p50(&old) as f64);
}
