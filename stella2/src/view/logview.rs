use cggeom::{box2, prelude::*, Box2};
use cgmath::Matrix3;
use std::{cell::Cell, ops::Range};
use tcw3::{
    pal,
    prelude::*,
    ui::{
        mixins::scrollwheel::ScrollAxisFlags,
        prelude::*,
        views::{table, table::LineTy},
    },
    uicore::{HView, HViewRef, HWndRef, SizeTraits, UpdateCtx, ViewFlags, ViewListener},
};

stella2_meta::designer_impl! {
    crate::view::logview::LogView
}

const GUTTER_WIDTH: f32 = 100.0;

impl LogView {
    fn init(&self) {
        // Set up the table model
        // TODO: hook up with a network layer
        let lipsum = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
                      Pellentesque ultricies diam sit amet ante auctor, et \
                      pretium orci molestie. Aenean facilisis justo ac tincidunt \
                      lobortis. Nulla molestie sem vel vehicula scelerisque. \
                      Quisque in viverra lacus, a suscipit lectus. Integer \
                      dignissim lacus neque, a condimentum tellus tempus ac. \
                      Praesent interdum, velit id mattis fringilla, tortor velit \
                      bibendum lorem, eget blandit augue nibh vel nunc. Duis ex \
                      ligula, porttitor ultricies velit vel, porta lacinia lectus. \
                      In pharetra auctor lorem, a efficitur tellus. Maecenas \
                      feugiat dapibus dolor quis dignissim. Quisque sed tortor \
                      sagittis, pretium mauris sit amet, ullamcorper turpis. \
                      Suspendisse potenti."
            .split_whitespace();
        let mk_lipsum = |num_words| lipsum.clone().take(num_words).collect::<Vec<_>>().join(" ");
        let rows = vec![
            Row::Date(chrono::NaiveDate::from_ymd(2018, 3, 1)),
            Row::LogItem("bob", "13:00", mk_lipsum(20)),
            Row::LogItem("alice", "13:32", mk_lipsum(25)),
            Row::LogItem("bob", "14:04", mk_lipsum(35)),
            Row::LogItem("alice", "14:36", mk_lipsum(5)),
            Row::LogItem("bob", "15:08", mk_lipsum(12)),
            Row::Date(chrono::NaiveDate::from_ymd(2018, 3, 2)),
            Row::LogItem("bob", "10:40", mk_lipsum(12)),
            Row::LogItem("alice", "11:12", mk_lipsum(15)),
            Row::LogItem("bob", "11:44", mk_lipsum(17)),
            Row::LogItem("alice", "14:16", mk_lipsum(40)),
            Row::LogItem("bob", "14:48", mk_lipsum(20)),
        ];

        {
            let mut edit = self.table().table().edit().unwrap();
            let num_rows = rows.len() as u64;
            edit.set_model(TableModelQuery {
                width: 100.0,
                dpi_scale: 1.0,
                row_visuals: rows
                    .iter()
                    .map(|row| RowVisual::from_row(row, 100.0, 1.0))
                    .collect(),
                rows,
            });
            edit.insert(LineTy::Row, 0..num_rows);
            edit.insert(LineTy::Col, 0..1);
        }
    }

    fn update_row_visuals(&self) {
        let dpi_scale = self.table().view().containing_wnd().unwrap().dpi_scale();

        let mut edit = self.table().table().edit().unwrap();
        let width = self.table().view().frame().size().x;

        let model: &mut TableModelQuery = edit.model_downcast_mut().unwrap();

        if (width - model.width).abs() < 0.1 && dpi_scale == model.dpi_scale {
            return;
        }

        model.width = width;
        model.dpi_scale = dpi_scale;

        model.row_visuals = model
            .rows
            .iter()
            .map(|row| RowVisual::from_row(row, width, dpi_scale))
            .collect();

        let num_rows = model.rows.len() as u64;
        edit.resize(LineTy::Row, 0..num_rows);
        edit.renew_subviews(LineTy::Row, 0..num_rows);
    }
}

struct TableModelQuery {
    row_visuals: Vec<RowVisual>,
    width: f32,
    dpi_scale: f32,
    rows: Vec<Row>,
}

impl table::TableModelQuery for TableModelQuery {
    fn new_view(&mut self, cell: table::CellIdx) -> (HView, Box<dyn table::CellCtrler>) {
        let hview = HView::new(Default::default());
        hview.set_listener(RowViewListener::new(
            self.row_visuals[cell[1] as usize].clone(),
        ));
        (hview, Box::new(()))
    }

    fn range_size(&mut self, line_ty: LineTy, range: Range<u64>, _approx: bool) -> f64 {
        match line_ty {
            LineTy::Row => self.row_visuals[range.start as usize..range.end as usize]
                .iter()
                .map(|v| v.height as f64)
                .sum(),

            // `TableFlags::GROW_LAST_COL` expands the column to cover the region.
            // The column needs some width for this flag to work.
            LineTy::Col => (range.end - range.start) as f64,
        }
    }
}

struct RowViewListener {
    layer: Cell<Option<pal::HLayer>>,
    row_visual: RowVisual,
}

impl RowViewListener {
    fn new(row_visual: RowVisual) -> Self {
        Self {
            layer: Cell::new(None),
            row_visual,
        }
    }
}

impl ViewListener for RowViewListener {
    fn mount(&self, wm: pal::Wm, hview: HViewRef<'_>, _: HWndRef<'_>) {
        self.layer.set(Some(wm.new_layer(pal::LayerAttrs {
            contents: Some(Some(self.row_visual.bmp.clone())),
            ..Default::default()
        })));

        hview.pend_update();
    }

    fn unmount(&self, wm: pal::Wm, _: HViewRef<'_>) {
        if let Some(hlayer) = self.layer.take() {
            wm.remove_layer(&hlayer);
        }
    }

    fn position(&self, _: pal::Wm, view: HViewRef<'_>) {
        view.pend_update();
    }

    fn update(&self, wm: pal::Wm, view: HViewRef<'_>, ctx: &mut UpdateCtx<'_>) {
        let layer = self.layer.take().unwrap();

        let view_frame = view.global_frame();

        wm.set_layer_attr(
            &layer,
            pal::LayerAttrs {
                bounds: Some(
                    self.row_visual
                        .bmp_bounds
                        .translate(view_frame.min - cgmath::Point2::new(0.0, 0.0)),
                ),
                ..Default::default()
            },
        );

        if ctx.layers().len() != 1 {
            ctx.set_layers(vec![layer.clone()]);
        }

        self.layer.set(Some(layer));
    }
}

// Mock-up log model
enum Row {
    Date(chrono::NaiveDate),
    LogItem(&'static str, &'static str, String),
}

#[derive(Clone)]
struct RowVisual {
    bmp: pal::Bitmap,
    bmp_bounds: Box2<f32>,
    height: f32,
}

impl RowVisual {
    #[allow(clippy::possible_missing_comma)]
    fn from_row(row: &Row, row_width: f32, dpi_scale: f32) -> Self {
        let v_margin = 3.0;
        let h_margin = 10.0;
        let scrollbar_margin = 12.0;

        let text = match row {
            Row::Date(d) => d.to_string(),
            Row::LogItem(name, _, body) => format!("{} @{} {}", name, name, body),
        };
        let char_style = pal::CharStyle::new(pal::CharStyleAttrs {
            ..Default::default()
        });
        let text_layout = pal::TextLayout::from_text(
            &text,
            &char_style,
            Some(row_width - h_margin * 2.0 - GUTTER_WIDTH - scrollbar_margin),
        );
        let layout_bounds = text_layout.layout_bounds();

        let row_height = layout_bounds.size().y.ceil() + v_margin * 2.0;
        let bmp_size = [
            (row_width * dpi_scale).ceil() as u32,
            (row_height * dpi_scale).ceil() as u32,
        ];
        let bmp_bounds = box2! {
            min: [0.0, 0.0],
            max: [bmp_size[0] as f32 / dpi_scale, bmp_size[1] as f32 / dpi_scale],
        };

        let bmp = {
            let mut builder = pal::BitmapBuilder::new(bmp_size);

            // Apply DPI scaling
            builder.mult_transform(Matrix3::from_scale_2d(dpi_scale));

            builder.set_fill_rgb([0.5, 0.5, 0.5, 0.05].into());
            builder.fill_rect(box2! {
                min: [0.0, 0.0],
                max: [GUTTER_WIDTH, row_height],
            });

            match row {
                Row::Date(_) => {
                    let y = row_height / 2.0;
                    let text_x_min =
                        (row_width - GUTTER_WIDTH - layout_bounds.size().x) / 2.0 + GUTTER_WIDTH;
                    let text_x_max = text_x_min + layout_bounds.size().x;

                    builder.begin_path();
                    builder.move_to([GUTTER_WIDTH, y].into());
                    builder.line_to([text_x_min - 8.0, y].into());
                    builder.move_to([row_width, y].into());
                    builder.line_to([text_x_max + 8.0, y].into());
                    builder.set_stroke_rgb([0.0, 0.0, 0.0, 0.2].into());
                    builder.stroke();

                    builder.draw_text(
                        &text_layout,
                        [text_x_min, v_margin - text_layout.layout_bounds().min.y].into(),
                        pal::RGBAF32::new(0.0, 0.0, 0.0, 1.0),
                    );
                }
                Row::LogItem(author, time, _) => {
                    let y = v_margin - text_layout.layout_bounds().min.y;
                    builder.draw_text(
                        &text_layout,
                        [h_margin + GUTTER_WIDTH, y].into(),
                        pal::RGBAF32::new(0.0, 0.0, 0.0, 1.0),
                    );

                    // Avatar
                    let avatar_size = 16.0;
                    if *author == "bob" {
                        builder.set_fill_rgb([0.8, 0.4, 0.3, 1.0].into());
                    } else {
                        builder.set_fill_rgb([0.1, 0.6, 0.6, 1.0].into());
                    }
                    builder.begin_path();
                    builder.rounded_rect(
                        box2! {
                            top_right: [GUTTER_WIDTH - 6.0, y],
                            size: [avatar_size; 2],
                        },
                        [[2.0; 2]; 4],
                    );
                    builder.fill();

                    // Time
                    let time_text_layout = pal::TextLayout::from_text(time, &char_style, None);
                    builder.draw_text(
                        &time_text_layout,
                        [
                            GUTTER_WIDTH
                                - time_text_layout.layout_bounds().max.x
                                - 12.0
                                - avatar_size,
                            y,
                        ]
                        .into(),
                        pal::RGBAF32::new(0.0, 0.0, 0.0, 0.6),
                    );
                }
            }

            builder.into_bitmap()
        };

        Self {
            bmp,
            bmp_bounds,
            height: row_height,
        }
    }
}
