//! JUnit XML report generator and merger for CI/CD pipelines
//!
//! Generates standard JUnit XML test reports compatible with:
//! - GitHub Actions (`dorny/test-reporter`, `action-junit-report`)
//! - GitLab CI / CD test reports
//! - CircleCI test metadata
//! - Jenkins JUnit plugin

use std::path::Path;

#[derive(Debug, Clone)]
pub enum TestCaseStatus {
    Passed,
    Failed { message: String, details: String },
    Skipped,
}

#[derive(Debug, Clone)]
pub struct JUnitTestCase {
    pub name:         String,
    pub classname:    String,
    pub time:         f64,
    pub cycles:       u64,
    pub record_bytes: u64,
    pub status:       TestCaseStatus,
    pub stdout:       String,
    pub stderr:       String,
}

#[derive(Debug, Clone)]
pub struct JUnitTestSuite {
    pub name:  String,
    pub time:  f64,
    pub cases: Vec<JUnitTestCase>,
}

impl JUnitTestSuite {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name:  name.into(),
            time:  0.0,
            cases: Vec::new(),
        }
    }

    pub fn to_xml(&self) -> String {
        let (failures, skipped) = self.cases.iter().fold((0, 0), |(failures, skipped), case| match &case.status {
            TestCaseStatus::Failed { .. } => (failures + 1, skipped),
            TestCaseStatus::Skipped => (failures, skipped + 1),
            TestCaseStatus::Passed => (failures, skipped),
        });

        let mut out = String::new();
        out.push_str(&format!(
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" errors=\"0\" skipped=\"{}\" time=\"{:.3}\">\n",
            escape_xml(&self.name),
            self.cases.len(),
            failures,
            skipped,
            self.time
        ));

        for c in &self.cases {
            out.push_str(&format!(
                "    <testcase name=\"{}\" classname=\"{}\" time=\"{:.3}\">\n",
                escape_xml(&c.name),
                escape_xml(&c.classname),
                c.time
            ));

            out.push_str("      <properties>\n");
            out.push_str(&format!("        <property name=\"cycles\" value=\"{}\"/>\n", c.cycles));
            if c.record_bytes > 0 {
                out.push_str(&format!(
                    "        <property name=\"trace_bytes\" value=\"{}\"/>\n",
                    c.record_bytes
                ));
            }
            out.push_str("      </properties>\n");

            match &c.status {
                TestCaseStatus::Passed => {},
                TestCaseStatus::Failed { message, details } => {
                    out.push_str(&format!(
                        "      <failure message=\"{}\">{}</failure>\n",
                        escape_xml(message),
                        escape_xml(details)
                    ));
                },
                TestCaseStatus::Skipped => {
                    out.push_str("      <skipped/>\n");
                },
            }

            if !c.stdout.is_empty() {
                out.push_str(&format!("      <system-out>{}</system-out>\n", escape_xml(&c.stdout)));
            }
            if !c.stderr.is_empty() {
                out.push_str(&format!("      <system-err>{}</system-err>\n", escape_xml(&c.stderr)));
            }

            out.push_str("    </testcase>\n");
        }

        out.push_str("  </testsuite>");
        out
    }
}

pub fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Keep valid XML text characters
            '\t' | '\n' | '\r' => out.push(c),
            ch if (ch as u32) < 0x20 => {
                // Ignore other control characters
            },
            other => out.push(other),
        }
    }
    out
}

pub fn write_or_merge_junit_xml(path: &Path, new_suite: JUnitTestSuite) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }

    // Suites written by earlier runs are carried over verbatim and only the roll-up is
    // recomputed, so their counts are read back out of the serialized form.
    let mut suites = match std::fs::read_to_string(path) {
        Ok(content) => parse_existing_suites(&content),
        Err(_) => Vec::new(),
    };

    let new_xml = new_suite.to_xml();
    match suites.iter_mut().find(|(name, _)| *name == new_suite.name) {
        Some((_, xml)) => *xml = new_xml,
        None => suites.push((new_suite.name, new_xml)),
    }

    let (tests, failures, errors, skipped, total_time) = suites.iter().fold((0, 0, 0, 0, 0.0), |totals, (_, xml)| {
        (
            totals.0 + extract_attr_usize(xml, "tests").unwrap_or_default(),
            totals.1 + extract_attr_usize(xml, "failures").unwrap_or_default(),
            totals.2 + extract_attr_usize(xml, "errors").unwrap_or_default(),
            totals.3 + extract_attr_usize(xml, "skipped").unwrap_or_default(),
            totals.4 + extract_attr_f64(xml, "time").unwrap_or_default(),
        )
    });

    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<testsuites name=\"zkvm-test\" tests=\"{}\" failures=\"{}\" errors=\"{}\" skipped=\"{}\" time=\"{:.3}\">\n",
        tests, failures, errors, skipped, total_time,
    ));
    for (_, xml) in &suites {
        out.push_str(xml);
        out.push('\n');
    }
    out.push_str("</testsuites>\n");

    let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp_path, out).map_err(|e| format!("failed to write JUnit XML to {}: {e}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, path).map_err(|e| format!("failed to finalize JUnit XML to {}: {e}", path.display()))
}

fn parse_existing_suites(xml: &str) -> Vec<(String, String)> {
    let mut suites = Vec::new();
    let mut search_idx = 0;

    while let Some(start) = xml[search_idx..].find("<testsuite ") {
        let actual_start = search_idx + start;
        let Some(end) = xml[actual_start..].find("</testsuite>") else {
            break;
        };
        let actual_end = actual_start + end + "</testsuite>".len();
        let suite_str = xml[actual_start..actual_end].to_string();

        let name = extract_attr_str(&suite_str, "name").unwrap_or_default();
        suites.push((name, suite_str));

        search_idx = actual_end;
    }

    suites
}

fn extract_attr_str(xml: &str, attr: &str) -> Option<String> {
    let pattern = format!("{attr}=\"");
    let start = xml.find(&pattern)? + pattern.len();
    let end = xml[start..].find('"')? + start;
    Some(xml[start..end].to_string())
}

fn extract_attr_usize(xml: &str, attr: &str) -> Option<usize> {
    extract_attr_str(xml, attr).and_then(|s| s.parse().ok())
}

fn extract_attr_f64(xml: &str, attr: &str) -> Option<f64> {
    extract_attr_str(xml, attr).and_then(|s| s.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_junit_suite_to_xml() {
        let mut suite = JUnitTestSuite::new("zkpow_core");
        suite.time = 1.25;
        suite.cases.push(JUnitTestCase {
            name:         "tests::test_pow".to_string(),
            classname:    "zkpow_core::tests".to_string(),
            time:         0.25,
            cycles:       50_000,
            record_bytes: 1024,
            status:       TestCaseStatus::Passed,
            stdout:       "hello stdout".to_string(),
            stderr:       String::new(),
        });
        suite.cases.push(JUnitTestCase {
            name:         "tests::test_failed".to_string(),
            classname:    "zkpow_core::tests".to_string(),
            time:         0.10,
            cycles:       10_000,
            record_bytes: 512,
            status:       TestCaseStatus::Failed {
                message: "budget exceeded".to_string(),
                details: "cycle budget exceeded: 10,000 > 5,000".to_string(),
            },
            stdout:       String::new(),
            stderr:       "stack trace...".to_string(),
        });

        let xml = suite.to_xml();
        assert!(xml.contains("name=\"zkpow_core\""));
        assert!(xml.contains("tests=\"2\""));
        assert!(xml.contains("failures=\"1\""));
        assert!(xml.contains("<property name=\"cycles\" value=\"50000\"/>"));
        assert!(
            xml.contains("<failure message=\"budget exceeded\">cycle budget exceeded: 10,000 &gt; 5,000</failure>")
        );
        assert!(xml.contains("<system-out>hello stdout</system-out>"));
        assert!(xml.contains("<system-err>stack trace...</system-err>"));
    }

    #[test]
    fn test_write_and_merge_junit_xml() {
        let tmp_dir = std::env::temp_dir().join("zkvm_junit_test");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        let xml_path = tmp_dir.join("results.xml");

        let mut suite1 = JUnitTestSuite::new("suite_one");
        suite1.time = 0.5;
        suite1.cases.push(JUnitTestCase {
            name:         "test1".to_string(),
            classname:    "suite_one".to_string(),
            time:         0.5,
            cycles:       1000,
            record_bytes: 100,
            status:       TestCaseStatus::Passed,
            stdout:       String::new(),
            stderr:       String::new(),
        });

        write_or_merge_junit_xml(&xml_path, suite1).unwrap();

        let mut suite2 = JUnitTestSuite::new("suite_two");
        suite2.time = 1.0;
        suite2.cases.push(JUnitTestCase {
            name:         "test2".to_string(),
            classname:    "suite_two".to_string(),
            time:         1.0,
            cycles:       2000,
            record_bytes: 200,
            status:       TestCaseStatus::Passed,
            stdout:       String::new(),
            stderr:       String::new(),
        });

        write_or_merge_junit_xml(&xml_path, suite2).unwrap();

        let merged = std::fs::read_to_string(&xml_path).unwrap();
        assert!(merged.contains("tests=\"2\""));
        assert!(merged.contains("name=\"suite_one\""));
        assert!(merged.contains("name=\"suite_two\""));

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_junit_skipped_case_and_escaping() {
        let mut suite = JUnitTestSuite::new("suite_escaping");
        suite.cases.push(JUnitTestCase {
            name:         "test::special_<>&\"'".to_string(),
            classname:    "suite_escaping".to_string(),
            time:         0.0,
            cycles:       0,
            record_bytes: 0,
            status:       TestCaseStatus::Skipped,
            stdout:       "out with <tag> & entity".to_string(),
            stderr:       "err with 'quotes' and \"double\"".to_string(),
        });

        let xml = suite.to_xml();
        assert!(xml.contains("skipped=\"1\""));
        assert!(xml.contains("<skipped/>"));
        assert!(xml.contains("name=\"test::special_&lt;&gt;&amp;&quot;&apos;\""));
        assert!(xml.contains("<system-out>out with &lt;tag&gt; &amp; entity</system-out>"));
        assert!(xml.contains("<system-err>err with &apos;quotes&apos; and &quot;double&quot;</system-err>"));
    }

    #[test]
    fn test_junit_replace_existing_suite() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path();

        let mut suite1 = JUnitTestSuite::new("suite_a");
        suite1.cases.push(JUnitTestCase {
            name:         "test1".to_string(),
            classname:    "suite_a".to_string(),
            time:         0.1,
            cycles:       100,
            record_bytes: 50,
            status:       TestCaseStatus::Passed,
            stdout:       String::new(),
            stderr:       String::new(),
        });
        write_or_merge_junit_xml(path, suite1).unwrap();

        // Write an updated suite_a with 2 tests
        let mut suite1_updated = JUnitTestSuite::new("suite_a");
        suite1_updated.cases.push(JUnitTestCase {
            name:         "test1".to_string(),
            classname:    "suite_a".to_string(),
            time:         0.1,
            cycles:       100,
            record_bytes: 50,
            status:       TestCaseStatus::Passed,
            stdout:       String::new(),
            stderr:       String::new(),
        });
        suite1_updated.cases.push(JUnitTestCase {
            name:         "test2".to_string(),
            classname:    "suite_a".to_string(),
            time:         0.2,
            cycles:       200,
            record_bytes: 60,
            status:       TestCaseStatus::Passed,
            stdout:       String::new(),
            stderr:       String::new(),
        });
        write_or_merge_junit_xml(path, suite1_updated).unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        // Should have 2 tests total, not 3 (no duplication of suite_a)
        assert!(content.contains("tests=\"2\""));
        assert_eq!(content.matches("<testsuite ").count(), 1);
    }
}
