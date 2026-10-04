use std::collections::HashMap;

use printfold_core::flow::{reflow, FlowRequest, Measurer};
use printfold_core::fonts::{FontRegistry, WidthCache};
use printfold_core::model::*;
use printfold_core::pdf::{generate_pdf, generate_test_page, prerender_plan, FileMeta, PdfContext};

fn out_dir() -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-output");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tiny_png() -> Vec<u8> {
    // 2x1 RGB PNG (red, blue).
    vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x7B, 0x40, 0xE8, 0xDD, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0x00, 0x04, 0xFF, 0x01, 0x07, 0x00, 0x01, 0xFF, 0xE2, 0x23, 0x9E, 0x59, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82]
}

fn sample_markdown() -> String {
    let para = "PrintFold lays out *markdown* into **signatures** for folding. Unicode works too: naïve café, “quotes”, em—dash, Ελληνικά, 日本語. ";
    format!(
        "# Chapter One\n\n{p}{p}A footnote here[^n].\n\n> A quoted ==highlight== and ~~struck~~ line.\n\n- first item\n- second item\n\n```\ncode block\n```\n\n![A caption under the picture](pic.png)\n\n{p}{p}{p}\n\n# Chapter Two\n\n{p}\n\n[^n]: The footnote text.",
        p = para
    )
}

#[test]
fn generates_booklet_and_sequential_pdfs() {
    let mut reg = FontRegistry::new();
    reg.load_system_fonts();
    if reg.face_count() == 0 {
        return;
    }
    let png = tiny_png();
    let mut project = ProjectSnapshot { name: "Test Booklet".into(), ..Default::default() };
    project.output_options.pages_per_signature = 8;
    project.header_footer.header.enabled = true;
    project.header_footer.header.verso.center = "PrintFold".into();
    project.header_footer.header.recto.center = "Header {{pageNumber}}".into();
    project.font_options.body.font_family = "DejaVu Serif".into();

    let mut sizes = HashMap::new();
    sizes.insert("pic.png".to_string(), (200.0, 100.0));
    let mut cache = WidthCache::default();
    let flowed = {
        let mut m = Measurer::new(&mut reg, &mut cache);
        reflow(&mut m, &FlowRequest { markdown: sample_markdown(), project: project.clone(), image_sizes: sizes })
    };
    project.signatures = flowed.signatures;

    // Give the last page a static item so the fallback item renderer runs.
    let last = project.signatures.last_mut().unwrap().spreads.last_mut().unwrap();
    if let Some(back) = last.verso.as_mut() {
        back.page_state = PageState::Static;
        back.items = Some(vec![
            PageItem { id: "r".into(), item_type: PageItemType::Shape, x: 20.0, y: 20.0, width: 100.0, height: 60.0, shape_type: Some("rectangle".into()), fill: Some(FillConfig { fill_type: "linearGradient".into(), linear_gradient: Some(LinearGradientConfig { angle: 0.0, stops: vec![GradientStop { offset: 0.0, color: "#3b82f6".into() }, GradientStop { offset: 1.0, color: "#8b5cf6".into() }] }), ..Default::default() }), rotation: Some(10.0), ..Default::default() },
            PageItem { id: "t".into(), item_type: PageItemType::Text, x: 20.0, y: 120.0, width: 200.0, height: 30.0, content: Some("Back cover text".into()), font_family: Some("DejaVu Sans".into()), font_size: Some(18.0), font_weight: Some("bold".into()), color: Some("#aa0000".into()), ..Default::default() },
            PageItem { id: "c".into(), item_type: PageItemType::Shape, x: 150.0, y: 200.0, width: 60.0, height: 60.0, shape_type: Some("circle".into()), fill_color: Some("#22aa22".into()), ..Default::default() },
        ]);
    }

    let files = vec![FileMeta { id: "img1".into(), name: "pic.png".into(), file_type: "image".into() }];
    let lookup = |id: &str| (id == "img1").then(|| png.clone());
    let pre: HashMap<u32, Vec<u8>> = HashMap::new();
    let ctx = PdfContext { project: &project, files: &files, pre_rendered: &pre, pre_rendered_backgrounds: &pre, file_bytes: &lookup };
    assert!(!prerender_plan(&project).overlay.is_empty());

    let pdf = generate_pdf(&mut reg, &ctx).expect("pdf");
    assert!(pdf.starts_with(b"%PDF"));
    std::fs::write(out_dir().join("booklet.pdf"), &pdf).unwrap();

    let mut seq = project.clone();
    seq.output_options.booklet_type = Some("doubleSided".into());
    let ctx = PdfContext { project: &seq, files: &files, pre_rendered: &pre, pre_rendered_backgrounds: &pre, file_bytes: &lookup };
    let pdf = generate_pdf(&mut reg, &ctx).expect("pdf");
    std::fs::write(out_dir().join("double-sided.pdf"), &pdf).unwrap();

    let mut test = project.clone();
    test.output_options.duplex_offset_x = Some(2.0 * 72.0 / 25.4);
    let ctx = PdfContext { project: &test, files: &files, pre_rendered: &pre, pre_rendered_backgrounds: &pre, file_bytes: &lookup };
    let pdf = generate_test_page(&mut reg, &ctx).expect("pdf");
    assert!(pdf.starts_with(b"%PDF"));
    std::fs::write(out_dir().join("test-page.pdf"), &pdf).unwrap();
}
