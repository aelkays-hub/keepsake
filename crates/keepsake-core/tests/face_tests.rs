//! The face models (ADR-040), run for real. Skipped when the models or
//! ONNX Runtime are not installed (`scripts/setup.ps1 -WithAi`,
//! `scripts/setup.sh --with-ai`).
//!
//! The corpus test also needs `KEEPSAKE_CORPUS` (photos with faces); run it
//! with `cargo test -p keepsake-core --test face_tests -- --include-ignored corpus`.

use image::{DynamicImage, Rgb, RgbImage};
use keepsake_core::ai::faces::{self, install, Face, FaceModels, EMBEDDING_LEN, SAME_PERSON};
use keepsake_core::ai::install as ai_install;

fn models() -> Option<FaceModels> {
    if !install::installed() || ai_install::find_runtime().is_none() {
        eprintln!("skipping: face models or ONNX Runtime not installed");
        return None;
    }
    Some(FaceModels::load_installed(false).unwrap())
}

#[test]
fn the_models_load_and_a_blank_picture_has_no_faces() {
    let Some(mut m) = models() else { return };
    let blank = DynamicImage::ImageRgb8(RgbImage::from_pixel(1024, 683, Rgb([128, 128, 128])));
    assert!(m.find(&blank).unwrap().is_empty());
}

#[test]
fn a_description_is_128_numbers_and_the_same_each_time() {
    let Some(mut m) = models() else { return };
    // Any picture and any landmarks: this checks the model's input and
    // output, not recognition.
    let mut img = RgbImage::new(400, 300);
    for (x, y, p) in img.enumerate_pixels_mut() {
        *p = Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8]);
    }
    let img = DynamicImage::ImageRgb8(img);
    let face = Face {
        x: 100.0,
        y: 50.0,
        w: 150.0,
        h: 180.0,
        landmarks: faces::align::TEMPLATE.map(|[x, y]| [100.0 + x * 1.3, 50.0 + y * 1.3]),
        score: 1.0,
    };
    let a = m.describe(&img, &face).unwrap();
    let b = m.describe(&img, &face).unwrap();
    assert_eq!(a.len(), EMBEDDING_LEN);
    let norm: f32 = a.values().iter().map(|v| v * v).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 1e-4);
    assert!(
        a.cosine(&b) > 0.9999,
        "the same input gives the same output"
    );
}

#[test]
fn the_gpu_and_the_cpu_agree() {
    if !install::installed() || ai_install::find_runtime().is_none() {
        return;
    }
    let (Ok(mut gpu), Ok(mut cpu)) = (
        FaceModels::load_installed(true),
        FaceModels::load_installed(false),
    ) else {
        return;
    };
    let img = DynamicImage::ImageRgb8(RgbImage::from_fn(300, 300, |x, y| {
        Rgb([(x % 200) as u8, (y % 180) as u8, 90])
    }));
    let face = Face {
        x: 50.0,
        y: 50.0,
        w: 150.0,
        h: 150.0,
        landmarks: faces::align::TEMPLATE.map(|[x, y]| [50.0 + x, 50.0 + y]),
        score: 1.0,
    };
    let (a, b) = (
        gpu.describe(&img, &face).unwrap(),
        cpu.describe(&img, &face).unwrap(),
    );
    assert!(a.cosine(&b) > 0.99, "{} on {:?}", a.cosine(&b), gpu.accel());
}

// ── Corpus: real photos ─────────────────────────────────────────────────────

fn photos(root: &std::path::Path, out: &mut Vec<std::path::PathBuf>, max: usize) {
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if out.len() >= max {
            return;
        }
        if p.is_dir() {
            photos(&p, out, max);
        } else if p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
        {
            out.push(p);
        }
    }
}

/// As the app would see it: the 1024 px thumbnail.
fn thumb(img: &DynamicImage, side: u32) -> DynamicImage {
    img.thumbnail(side, side)
}

#[test]
#[ignore = "requires KEEPSAKE_CORPUS and the face models"]
fn corpus_faces_are_found_and_match_themselves_at_another_size() {
    let Some(root) = std::env::var_os("KEEPSAKE_CORPUS").map(std::path::PathBuf::from) else {
        eprintln!("SKIP: KEEPSAKE_CORPUS not set");
        return;
    };
    let Some(mut m) = models() else { return };
    let mut files = Vec::new();
    photos(&root, &mut files, 300);
    let (mut looked, mut with_faces, mut faces_seen) = (0, 0, 0);
    let mut lowest = 1.0f32;
    for f in &files {
        let Ok(img) = image::open(f) else { continue };
        looked += 1;
        let big = thumb(&img, 1024);
        let found = m.find_and_describe(&big).unwrap();
        // Faces big enough to recognise (as the app will use them).
        let usable: Vec<_> = found.iter().filter(|(f, _)| f.w >= 40.0).collect();
        if usable.is_empty() {
            continue;
        }
        with_faces += 1;
        let (w, h) = (big.width() as f32, big.height() as f32);
        for (face, _) in &usable {
            assert!(face.score >= faces::MIN_SCORE);
            assert!(face.x > -face.w && face.x < w && face.y > -face.h && face.y < h);
            // The nose is in or near the box (a face in profile has it
            // just outside).
            let [nx, ny] = face.landmarks[2];
            let (mx, my) = (face.w / 2.0, face.h / 2.0);
            assert!(
                nx >= face.x - mx
                    && nx <= face.x + face.w + mx
                    && ny >= face.y - my
                    && ny <= face.y + face.h + my,
                "the nose is near the box: {face:?}"
            );
        }
        // The same photo, smaller: the best face is found again and
        // matches itself.
        let small = thumb(&img, 768);
        let k = small.width() as f32 / big.width() as f32;
        let again = m.find_and_describe(&small).unwrap();
        let (face, emb) = usable[0];
        let want = face.scaled(k);
        let Some((_, e2)) = again
            .iter()
            .find(|(g, _)| faces::detect::iou(g, &want) > 0.5)
        else {
            panic!(
                "{}: the face at {face:?} was not found at 768 px",
                f.display()
            );
        };
        let sim = emb.cosine(e2);
        lowest = lowest.min(sim);
        assert!(
            sim >= SAME_PERSON,
            "{}: the same face matched only {sim}",
            f.display()
        );
        faces_seen += usable.len();
        if with_faces >= 25 {
            break;
        }
    }
    eprintln!(
        "corpus: {looked} photos read, {with_faces} with faces, {faces_seen} faces; \
         lowest self-match {lowest:.3}"
    );
    if with_faces == 0 {
        eprintln!("SKIP: no faces in the first {} corpus photos", files.len());
    }
}
