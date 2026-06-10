use std::error::Error;
use std::fmt::{self, Write};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StageSample {
    rss_kb: u64,
    live_kb: u64,
}

impl StageSample {
    pub const fn new(rss_kb: u64, live_kb: u64) -> Self {
        Self { rss_kb, live_kb }
    }

    pub const fn rss_kb(&self) -> u64 {
        self.rss_kb
    }

    pub const fn live_kb(&self) -> u64 {
        self.live_kb
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StageMetric {
    RssKb,
    LiveKb,
}

impl StageMetric {
    fn value(self, sample: StageSample) -> u64 {
        match self {
            Self::RssKb => sample.rss_kb(),
            Self::LiveKb => sample.live_kb(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct StageCountMismatch {
    expected: usize,
    actual: usize,
}

impl StageCountMismatch {
    pub const fn expected(&self) -> usize {
        self.expected
    }

    pub const fn actual(&self) -> usize {
        self.actual
    }
}

impl fmt::Display for StageCountMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "expected {} stages per iteration, got {}",
            self.expected, self.actual
        )
    }
}

impl Error for StageCountMismatch {}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DeltaSummary {
    min_delta: i64,
    max_delta: i64,
    avg_delta: f64,
}

impl DeltaSummary {
    fn min_display(self) -> String {
        self.min_delta.to_string()
    }

    fn max_display(self) -> String {
        self.max_delta.to_string()
    }

    fn avg_display(self) -> String {
        format!("{:.1}", self.avg_delta)
    }
}

pub struct StageHistory {
    stage_labels: Vec<String>,
    rows: Vec<Vec<StageSample>>,
}

impl StageHistory {
    pub fn new<I, S>(stage_labels: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            stage_labels: stage_labels.into_iter().map(Into::into).collect(),
            rows: Vec::new(),
        }
    }

    pub fn iterations(&self) -> usize {
        self.rows.len()
    }

    pub fn sample(&self, iteration: usize, stage_index: usize) -> Option<StageSample> {
        self.rows
            .get(iteration)
            .and_then(|row| row.get(stage_index))
            .copied()
    }

    pub fn push_iteration<I>(&mut self, stages: I) -> Result<(), StageCountMismatch>
    where
        I: IntoIterator<Item = StageSample>,
    {
        let row: Vec<StageSample> = stages.into_iter().collect();
        if row.len() != self.stage_labels.len() {
            return Err(StageCountMismatch {
                expected: self.stage_labels.len(),
                actual: row.len(),
            });
        }
        self.rows.push(row);
        Ok(())
    }

    pub fn baseline_delta(&self, metric: StageMetric, stage_index: usize) -> Option<i64> {
        let first = self.sample(0, stage_index)?;
        let last = self.sample(self.iterations().checked_sub(1)?, stage_index)?;
        Some(metric.value(last) as i64 - metric.value(first) as i64)
    }

    pub fn render_table(&self, metric: StageMetric, title: &str) -> String {
        let iter_width = self
            .iterations()
            .saturating_sub(1)
            .to_string()
            .len()
            .max("Iter".len());
        let mut stage_widths: Vec<usize> =
            self.stage_labels.iter().map(|label| label.len()).collect();

        for row in &self.rows {
            for (index, sample) in row.iter().enumerate() {
                stage_widths[index] =
                    stage_widths[index].max(metric.value(*sample).to_string().len());
            }
        }

        let header = self.header_line(iter_width, &stage_widths);
        let row_width = header.len();
        let divider = "=".repeat(row_width);
        let separator = self.separator_line(iter_width, &stage_widths);

        let mut rendered = String::new();
        let _ = writeln!(rendered, "{divider}");
        let _ = writeln!(rendered, "{:^width$}", title, width = row_width);
        let _ = writeln!(rendered, "{divider}");
        let _ = writeln!(rendered, "{header}");
        let _ = writeln!(rendered, "{separator}");
        for (iteration, row) in self.rows.iter().enumerate() {
            let _ = write!(
                rendered,
                "{iteration:<iter_width$}",
                iter_width = iter_width
            );
            for (index, sample) in row.iter().enumerate() {
                let _ = write!(
                    rendered,
                    " | {:<width$}",
                    metric.value(*sample),
                    width = stage_widths[index]
                );
            }
            let _ = writeln!(rendered);
        }
        let _ = write!(rendered, "{divider}");

        let summary = self.render_delta_summary(metric);
        if !summary.is_empty() {
            let _ = writeln!(rendered);
            let _ = write!(rendered, "{summary}");
        }
        rendered
    }

    fn render_delta_summary(&self, metric: StageMetric) -> String {
        let summaries: Vec<Option<DeltaSummary>> = (0..self.stage_labels.len())
            .map(|stage_index| self.stage_delta_summary(metric, stage_index))
            .collect();

        let mut stage_widths: Vec<usize> =
            self.stage_labels.iter().map(|label| label.len()).collect();
        let mut min_cells = Vec::with_capacity(summaries.len());
        let mut max_cells = Vec::with_capacity(summaries.len());
        let mut avg_cells = Vec::with_capacity(summaries.len());

        for summary in &summaries {
            let min_cell = summary
                .map(|summary| summary.min_display())
                .unwrap_or_else(|| "n/a".to_string());
            let max_cell = summary
                .map(|summary| summary.max_display())
                .unwrap_or_else(|| "n/a".to_string());
            let avg_cell = summary
                .map(|summary| summary.avg_display())
                .unwrap_or_else(|| "n/a".to_string());

            min_cells.push(min_cell);
            max_cells.push(max_cell);
            avg_cells.push(avg_cell);
        }

        for index in 0..self.stage_labels.len() {
            stage_widths[index] = stage_widths[index]
                .max(min_cells[index].len())
                .max(max_cells[index].len())
                .max(avg_cells[index].len());
        }

        let metric_width = "Metric"
            .len()
            .max("Min delta".len())
            .max("Max delta".len())
            .max("Avg delta".len());

        let header = self.summary_header_line(metric_width, &stage_widths);
        let row_width = header.len();
        let divider = "=".repeat(row_width);
        let separator = self.summary_separator_line(metric_width, &stage_widths);

        let mut rendered = String::new();
        let _ = writeln!(rendered, "{divider}");
        let _ = writeln!(
            rendered,
            "{:^width$}",
            "ROW-TO-ROW DELTA SUMMARY",
            width = row_width
        );
        let _ = writeln!(rendered, "{divider}");
        let _ = writeln!(rendered, "{header}");
        let _ = writeln!(rendered, "{separator}");
        let _ = write!(
            rendered,
            "{:<metric_width$}",
            "Min delta",
            metric_width = metric_width
        );
        for (index, cell) in min_cells.iter().enumerate() {
            let _ = write!(rendered, " | {:<width$}", cell, width = stage_widths[index]);
        }
        let _ = writeln!(rendered);
        let _ = write!(
            rendered,
            "{:<metric_width$}",
            "Max delta",
            metric_width = metric_width
        );
        for (index, cell) in max_cells.iter().enumerate() {
            let _ = write!(rendered, " | {:<width$}", cell, width = stage_widths[index]);
        }
        let _ = writeln!(rendered);
        let _ = write!(
            rendered,
            "{:<metric_width$}",
            "Avg delta",
            metric_width = metric_width
        );
        for (index, cell) in avg_cells.iter().enumerate() {
            let _ = write!(rendered, " | {:<width$}", cell, width = stage_widths[index]);
        }
        let _ = writeln!(rendered);
        let _ = write!(rendered, "{divider}");
        rendered
    }

    fn stage_delta_summary(&self, metric: StageMetric, stage_index: usize) -> Option<DeltaSummary> {
        let mut deltas = self
            .rows
            .windows(2)
            .filter_map(|pair| {
                let start = pair.first()?.get(stage_index).copied()?;
                let end = pair.get(1)?.get(stage_index).copied()?;
                Some(metric.value(end) as i64 - metric.value(start) as i64)
            })
            .collect::<Vec<_>>();

        if deltas.is_empty() {
            return None;
        }

        deltas.sort_unstable();
        let min_delta = *deltas.first()?;
        let max_delta = *deltas.last()?;
        let avg_delta = deltas.iter().sum::<i64>() as f64 / deltas.len() as f64;
        Some(DeltaSummary {
            min_delta,
            max_delta,
            avg_delta,
        })
    }

    fn header_line(&self, iter_width: usize, stage_widths: &[usize]) -> String {
        let mut header = format!("{:<iter_width$}", "Iter", iter_width = iter_width);
        for (index, label) in self.stage_labels.iter().enumerate() {
            let _ = write!(header, " | {:<width$}", label, width = stage_widths[index]);
        }
        header
    }

    fn summary_header_line(&self, metric_width: usize, stage_widths: &[usize]) -> String {
        let mut header = format!("{:<metric_width$}", "Metric", metric_width = metric_width);
        for (index, label) in self.stage_labels.iter().enumerate() {
            let _ = write!(header, " | {:<width$}", label, width = stage_widths[index]);
        }
        header
    }

    fn separator_line(&self, iter_width: usize, stage_widths: &[usize]) -> String {
        let mut separator = "-".repeat(iter_width);
        for width in stage_widths {
            separator.push_str("-+-");
            separator.push_str(&"-".repeat(*width));
        }
        separator
    }

    fn summary_separator_line(&self, metric_width: usize, stage_widths: &[usize]) -> String {
        let mut separator = "-".repeat(metric_width);
        for width in stage_widths {
            separator.push_str("-+-");
            separator.push_str(&"-".repeat(*width));
        }
        separator
    }
}

#[cfg(test)]
mod tests {
    use super::{StageHistory, StageMetric, StageSample};

    #[test]
    fn render_table_includes_stage_values() {
        let mut history = StageHistory::new(["Stage 0", "Stage 1"]);
        history
            .push_iteration([StageSample::new(10, 20), StageSample::new(30, 40)])
            .unwrap();
        history
            .push_iteration([StageSample::new(15, 25), StageSample::new(35, 45)])
            .unwrap();

        let rendered = history.render_table(StageMetric::LiveKb, "LIVE");

        assert!(rendered.contains("Iter | Stage 0 | Stage 1"));
        assert!(rendered.contains("0    | 20"));
        assert!(rendered.contains("1    | 25"));
        assert!(rendered.contains("ROW-TO-ROW DELTA SUMMARY"));
        assert!(rendered.contains("Min delta"));
        assert!(rendered.contains("Max delta"));
        assert!(rendered.contains("Avg delta"));
        assert!(rendered.contains("5"));
        assert_eq!(history.baseline_delta(StageMetric::LiveKb, 0), Some(5));
    }
}
