use shotoshot_core::{Editor, Point, Shape};

#[test]
fn committing_new_text_uses_draft_color_and_one_undo() {
    let mut editor = Editor::default();
    let color = [40, 90, 180];
    let id = editor
        .add_text_measured_colored(
            Point { x: 5., y: 8. },
            "é😀".into(),
            Point { x: 60., y: 30. },
            color,
        )
        .unwrap();
    let object = editor.objects()[0].clone();
    assert_eq!(object.id, id);
    assert_eq!(object.style.color, color);
    assert_eq!(object.style.width, editor.style.width);
    editor.undo();
    assert!(editor.objects().is_empty());
    editor.redo();
    assert_eq!(editor.objects(), &[object]);
}

#[test]
fn text_and_color_commit_together_preserving_other_style_fields() {
    let mut editor = Editor::default();
    editor.style.width = 7.;
    editor.add_text(Point { x: 5., y: 8. }, "Original".into());
    let original = editor.objects()[0].clone();
    editor.style.width = 2.;
    let color = [10, 230, 110];
    editor.update_text_measured_colored(
        original.id,
        "Modifié\n日本".into(),
        Point { x: 95., y: 60. },
        color,
    );
    let edited = editor.objects()[0].clone();
    assert_eq!(edited.style.color, color);
    assert_eq!(edited.style.width, original.style.width);
    assert!(
        matches!(&edited.shape, Shape::Text { size, text, .. } if *size == 24. && text == "Modifié\n日本")
    );
    editor.undo();
    assert_eq!(editor.objects(), &[original]);
    editor.redo();
    assert_eq!(editor.objects(), &[edited]);
    editor.undo();
    editor.undo();
    assert!(editor.objects().is_empty());
}
