use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A single task report stored on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReport {
    pub id: String,
    pub timestamp: String,
    pub project_root: String,
    pub task_type: String,
    pub summary: String,
    #[serde(default)]
    pub tools_used: Vec<String>,
    #[serde(default)]
    pub files_modified: Vec<String>,
    #[serde(default)]
    pub issues_found: Vec<String>,
    #[serde(default)]
    pub improvement_suggestions: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Aggregated result from synthesizing multiple reports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisResult {
    pub total_reports: usize,
    pub reports_analyzed: usize,
    pub date_range: Option<DateRange>,
    pub task_type_breakdown: HashMap<String, u32>,
    pub frequently_modified_files: Vec<FileFrequency>,
    pub recurring_issues: Vec<IssueFrequency>,
    pub improvement_suggestions: Vec<SuggestionFrequency>,
    pub tools_usage: Vec<ToolUsageEntry>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFrequency {
    pub file_path: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueFrequency {
    pub issue: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionFrequency {
    pub suggestion: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUsageEntry {
    pub tool: String,
    pub count: u32,
}

fn reports_dir(project_root: &Path) -> PathBuf {
    project_root.join(".cortex").join("reports")
}

fn generate_id() -> String {
    let now = chrono::Utc::now();
    let ts = now.format("%Y%m%d-%H%M%S");
    let random_part: String = rand::random::<u32>().to_string().chars().take(6).collect();
    format!("{}-{}", ts, random_part)
}

pub fn save_report(project_root: &Path, mut report: TaskReport) -> crate::error::Result<PathBuf> {
    let dir = reports_dir(project_root);
    std::fs::create_dir_all(&dir)?;

    report.id = generate_id();
    report.timestamp = chrono::Utc::now().to_rfc3339();
    report.project_root = project_root.to_string_lossy().to_string();

    let file_name = format!("{}.json", report.id);
    let file_path = dir.join(&file_name);

    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| crate::error::CortexError::Config(e.to_string()))?;

    std::fs::write(&file_path, json)?;

    Ok(file_path)
}

pub fn load_reports(project_root: &Path, limit: usize) -> crate::error::Result<Vec<TaskReport>> {
    let dir = reports_dir(project_root);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut reports: Vec<TaskReport> = Vec::new();

    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let content = std::fs::read_to_string(&path)?;
            if let Ok(report) = serde_json::from_str::<TaskReport>(&content) {
                reports.push(report);
            }
        }
    }

    // Sort newest first
    reports.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    reports.truncate(limit);

    Ok(reports)
}

pub fn count_reports(project_root: &Path) -> crate::error::Result<usize> {
    let dir = reports_dir(project_root);
    if !dir.exists() {
        return Ok(0);
    }

    let count = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|e| e.to_str()) == Some("json"))
        .count();

    Ok(count)
}

pub fn synthesize(reports: &[TaskReport], total_count: usize) -> SynthesisResult {
    let reports_analyzed = reports.len();

    // Date range
    let date_range = if reports.is_empty() {
        None
    } else {
        let timestamps: Vec<&str> = reports.iter().map(|r| r.timestamp.as_str()).collect();
        let min = timestamps.iter().min().unwrap().to_string();
        let max = timestamps.iter().max().unwrap().to_string();
        Some(DateRange { from: min, to: max })
    };

    // Task type breakdown
    let mut task_type_counts: HashMap<String, u32> = HashMap::new();
    for r in reports {
        *task_type_counts.entry(r.task_type.clone()).or_insert(0) += 1;
    }

    // File frequency
    let mut file_counts: HashMap<String, u32> = HashMap::new();
    for r in reports {
        for f in &r.files_modified {
            *file_counts.entry(f.clone()).or_insert(0) += 1;
        }
    }
    let mut frequently_modified_files: Vec<FileFrequency> = file_counts
        .into_iter()
        .map(|(file_path, count)| FileFrequency { file_path, count })
        .collect();
    frequently_modified_files.sort_by_key(|b| std::cmp::Reverse(b.count));
    frequently_modified_files.truncate(10);

    // Issue frequency
    let mut issue_counts: HashMap<String, u32> = HashMap::new();
    for r in reports {
        for issue in &r.issues_found {
            *issue_counts.entry(issue.clone()).or_insert(0) += 1;
        }
    }
    let mut recurring_issues: Vec<IssueFrequency> = issue_counts
        .into_iter()
        .map(|(issue, count)| IssueFrequency { issue, count })
        .collect();
    recurring_issues.sort_by_key(|b| std::cmp::Reverse(b.count));
    recurring_issues.truncate(20);

    // Suggestion frequency
    let mut suggestion_counts: HashMap<String, u32> = HashMap::new();
    for r in reports {
        for s in &r.improvement_suggestions {
            *suggestion_counts.entry(s.clone()).or_insert(0) += 1;
        }
    }
    let mut improvement_suggestions: Vec<SuggestionFrequency> = suggestion_counts
        .into_iter()
        .map(|(suggestion, count)| SuggestionFrequency { suggestion, count })
        .collect();
    improvement_suggestions.sort_by_key(|b| std::cmp::Reverse(b.count));
    improvement_suggestions.truncate(20);

    // Tool usage
    let mut tool_counts: HashMap<String, u32> = HashMap::new();
    for r in reports {
        for t in &r.tools_used {
            *tool_counts.entry(t.clone()).or_insert(0) += 1;
        }
    }
    let mut tools_usage: Vec<ToolUsageEntry> = tool_counts
        .into_iter()
        .map(|(tool, count)| ToolUsageEntry { tool, count })
        .collect();
    tools_usage.sort_by_key(|b| std::cmp::Reverse(b.count));

    // Generate narrative summary
    let summary = generate_summary(
        total_count,
        reports_analyzed,
        &task_type_counts,
        &frequently_modified_files,
        &recurring_issues,
        &improvement_suggestions,
    );

    SynthesisResult {
        total_reports: total_count,
        reports_analyzed,
        date_range,
        task_type_breakdown: task_type_counts,
        frequently_modified_files,
        recurring_issues,
        improvement_suggestions,
        tools_usage,
        summary,
    }
}

fn generate_summary(
    total: usize,
    analyzed: usize,
    task_types: &HashMap<String, u32>,
    top_files: &[FileFrequency],
    top_issues: &[IssueFrequency],
    top_suggestions: &[SuggestionFrequency],
) -> String {
    let mut parts = Vec::new();

    parts.push(format!("Analyzed {} of {} total reports.", analyzed, total));

    if !task_types.is_empty() {
        let breakdown: Vec<String> = task_types
            .iter()
            .map(|(k, v)| format!("{} {}(s)", v, k))
            .collect();
        parts.push(format!("Task types: {}.", breakdown.join(", ")));
    }

    if !top_files.is_empty() {
        let files: Vec<String> = top_files
            .iter()
            .take(3)
            .map(|f| format!("{} ({}x)", f.file_path, f.count))
            .collect();
        parts.push(format!("Most modified files: {}.", files.join(", ")));
    }

    if !top_issues.is_empty() {
        let issues: Vec<String> = top_issues
            .iter()
            .take(3)
            .map(|i| format!("\"{}\" ({}x)", i.issue, i.count))
            .collect();
        parts.push(format!("Recurring issues: {}.", issues.join("; ")));
    }

    if !top_suggestions.is_empty() {
        let suggestions: Vec<String> = top_suggestions
            .iter()
            .take(3)
            .map(|s| format!("\"{}\" ({}x)", s.suggestion, s.count))
            .collect();
        parts.push(format!(
            "Top improvement suggestions: {}.",
            suggestions.join("; ")
        ));
    }

    parts.join(" ")
}
