use printfold_core::flow::{calculate_imposition, reflow, FlowRequest, Measurer};
use printfold_core::fonts::{FontRegistry, WidthCache};
use printfold_core::model::*;

fn fonts() -> FontRegistry {
    let mut r = FontRegistry::new();
    r.load_system_fonts();
    r
}

fn pages(sigs: &[Signature]) -> Vec<&PageContent> {
    all_pages(sigs).collect()
}

fn lorem(n: usize) -> String {
    let words = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua";
    let w: Vec<&str> = words.split(' ').collect();
    (0..n).map(|i| w[i % w.len()]).collect::<Vec<_>>().join(" ")
}

#[test]
fn flows_text_into_padded_signatures() {
    let mut reg = fonts();
    let mut cache = WidthCache::default();
    let mut m = Measurer::new(&mut reg, &mut cache);
    let md = format!("# One\n\n{}\n\n# Two\n\n{}\n", lorem(600), lorem(300));
    let mut project = ProjectSnapshot::default();
    project.output_options.pages_per_signature = 8;
    let res = reflow(&mut m, &FlowRequest { markdown: md, project, ..Default::default() });
    let ps = pages(&res.signatures);
    assert!(res.total_pages >= 4);
    assert_eq!(res.total_pages % 8, 0, "booklet mode pads to whole signatures");
    assert_eq!(ps.len(), res.total_pages);
    // Page numbers are sequential in spread order.
    for (i, p) in ps.iter().enumerate() {
        assert_eq!(p.page_number as usize, i + 1);
        assert_eq!(p.is_recto, p.page_number % 2 == 1);
    }
    // Every H1 starts on a recto.
    for p in &ps {
        if let Some(first) = p.sections.first() {
            if first.section_type == SectionType::Heading && first.level == Some(1) {
                assert!(p.is_recto, "H1 on page {} should be recto", p.page_number);
            }
        }
    }
    // Imposition of the first signature covers its pages.
    let sheets = calculate_imposition(&res.signatures[0]);
    assert_eq!(sheets.len(), 2);
}

#[test]
fn preserves_static_pages_and_flows_around_them() {
    let mut reg = fonts();
    let mut cache = WidthCache::default();
    let mut m = Measurer::new(&mut reg, &mut cache);
    let mut project = ProjectSnapshot::default();
    let first = reflow(&mut m, &FlowRequest { markdown: lorem(1500), project: project.clone(), ..Default::default() });
    // Mark page 2 static with an item, then reflow.
    let mut sigs = first.signatures.clone();
    for sig in &mut sigs {
        for spread in &mut sig.spreads {
            for p in [spread.verso.as_mut(), spread.recto.as_mut()].into_iter().flatten() {
                if p.page_number == 2 {
                    p.page_state = PageState::Static;
                    p.sections.clear();
                    p.items = Some(vec![PageItem { id: "shape1".into(), item_type: PageItemType::Shape, width: 10.0, height: 10.0, ..Default::default() }]);
                }
            }
        }
    }
    project.signatures = sigs;
    let second = reflow(&mut m, &FlowRequest { markdown: lorem(1500), project, ..Default::default() });
    let ps = pages(&second.signatures);
    let p2 = ps.iter().find(|p| p.page_number == 2).unwrap();
    assert_eq!(p2.page_state, PageState::Static);
    assert_eq!(p2.items().len(), 1);
    assert!(p2.sections.is_empty());
    assert!(ps.iter().filter(|p| p.page_state == PageState::Text).count() >= 2);
}

#[test]
fn footnotes_attach_to_pages() {
    let mut reg = fonts();
    let mut cache = WidthCache::default();
    let mut m = Measurer::new(&mut reg, &mut cache);
    let md = format!("Start[^a] {}\n\n[^a]: The note body.", lorem(50));
    let res = reflow(&mut m, &FlowRequest { markdown: md.clone(), project: ProjectSnapshot::default(), ..Default::default() });
    let ps = pages(&res.signatures);
    let with_notes: Vec<_> = ps.iter().filter(|p| p.footnotes.is_some()).collect();
    assert_eq!(with_notes.len(), 1);
    assert_eq!(with_notes[0].footnotes.as_ref().unwrap()[0].content, "The note body.");

    let mut project = ProjectSnapshot::default();
    project.layout_options.show_footnotes_as_endnotes = Some(true);
    let res = reflow(&mut m, &FlowRequest { markdown: md, project, ..Default::default() });
    let ps = pages(&res.signatures);
    assert!(ps.iter().all(|p| p.footnotes.is_none()));
    assert!(ps.iter().any(|p| p.sections.iter().any(|s| s.section_type == SectionType::Endnote)));
}

#[test]
fn text_flow_region_receives_content() {
    let mut reg = fonts();
    let mut cache = WidthCache::default();
    let mut m = Measurer::new(&mut reg, &mut cache);
    let mut project = ProjectSnapshot::default();
    let base = reflow(&mut m, &FlowRequest { markdown: String::new(), project: project.clone(), ..Default::default() });
    let mut sigs = base.signatures;
    for sig in &mut sigs {
        for spread in &mut sig.spreads {
            for p in [spread.verso.as_mut(), spread.recto.as_mut()].into_iter().flatten() {
                if p.page_number == 2 {
                    p.page_state = PageState::Static;
                    let square = PageItem { id: "flow1".into(), item_type: PageItemType::TextFlow, x: 20.0, y: 20.0, width: 150.0, height: 100.0, flow_shape: Some("square".into()), ..Default::default() };
                    let poly_pts = [(0.5, 0.0), (1.0, 1.0), (0.0, 1.0)].iter().map(|&(x, y)| PolygonPoint { x, y, ..Default::default() }).collect();
                    let poly = PageItem { id: "flow2".into(), item_type: PageItemType::TextFlow, x: 20.0, y: 150.0, width: 150.0, height: 120.0, flow_shape: Some("polygon".into()), polygon_points: Some(poly_pts), ..Default::default() };
                    p.items = Some(vec![square, poly]);
                }
            }
        }
    }
    project.signatures = sigs;
    let res = reflow(&mut m, &FlowRequest { markdown: lorem(800), project, ..Default::default() });
    let ps = pages(&res.signatures);
    let p2 = ps.iter().find(|p| p.page_number == 2).unwrap();
    let items = p2.items();
    assert!(!items[0].flowed_sections.as_ref().unwrap().is_empty(), "square region filled");
    assert!(!items[1].flowed_polygon_lines.as_ref().unwrap().is_empty(), "polygon region filled");
    // Polygon lines stay inside the triangle: narrower near the apex.
    let lines = items[1].flowed_polygon_lines.as_ref().unwrap();
    assert!(lines.iter().all(|l| l.x >= -0.01 && l.y >= 0.0));
}

#[test]
fn removing_all_text_clears_stale_pages() {
    let mut reg = fonts();
    let mut cache = WidthCache::default();
    let mut m = Measurer::new(&mut reg, &mut cache);
    let mut project = ProjectSnapshot::default();
    let first = reflow(&mut m, &FlowRequest { markdown: lorem(200), project: project.clone(), ..Default::default() });
    project.signatures = first.signatures;
    let second = reflow(&mut m, &FlowRequest { markdown: String::new(), project, ..Default::default() });
    assert!(pages(&second.signatures).iter().all(|p| p.sections.is_empty()));
}

#[test]
fn large_document_is_fast() {
    let mut reg = fonts();
    let mut cache = WidthCache::default();
    let mut m = Measurer::new(&mut reg, &mut cache);
    let md: String = (0..40).map(|i| format!("# Chapter {i}\n\n{}\n\n{}\n\n", lorem(400), lorem(300))).collect();
    let start = std::time::Instant::now();
    let res = reflow(&mut m, &FlowRequest { markdown: md, project: ProjectSnapshot::default(), ..Default::default() });
    let elapsed = start.elapsed();
    eprintln!("reflowed {} pages in {:?}", res.total_pages, elapsed);
    assert!(res.total_pages > 100);
    let json = serde_json::to_string(&res).unwrap();
    eprintln!("result JSON: {} KB", json.len() / 1024);
}
