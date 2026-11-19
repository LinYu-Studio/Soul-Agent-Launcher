// Soul Agent Launcher - 知识库模块
// 模型无关 RAG：文件解析 → 分块 → 嵌入（可选）→ 检索 → 上下文注入

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 文档分块
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub doc_id: String,
    pub doc_path: String,
    pub doc_name: String,
    pub chunk_index: usize,
    pub text: String,
    /// 字符级位置（文件中的起始偏移）
    pub char_start: usize,
    pub char_end: usize,
    /// 来源："folder" 或 "temp"
    pub source: String,
    /// 嵌入向量（可选，由嵌入服务器生成）
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
}

/// 知识库状态
pub struct KnowledgeState {
    /// 所有文档分块（内存中）
    pub chunks: Mutex<Vec<DocumentChunk>>,
    /// 已添加的文件夹路径（持久化）
    pub folders: Mutex<Vec<String>>,
    /// 是否启用知识库检索
    pub enabled: Mutex<bool>,
    /// 嵌入模型是否可用（嵌入服务器运行中）
    pub embedding_available: Mutex<bool>,
}

impl KnowledgeState {
    pub fn new() -> Self {
        KnowledgeState {
            chunks: Mutex::new(Vec::new()),
            folders: Mutex::new(Vec::new()),
            enabled: Mutex::new(true),
            embedding_available: Mutex::new(false),
        }
    }
}

/// 解析文本文件
fn parse_text_file(path: &Path) -> Result<String, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("读取文件失败: {}", e))?;
    Ok(content)
}

/// 分块：按段落 + 字符数切割
fn chunk_text(text: &str, doc_id: &str, doc_path: &str, doc_name: &str, source: &str) -> Vec<DocumentChunk> {
    let max_chunk = 800;  // 每块最多字符
    let overlap = 100;     // 重叠字符
    let mut chunks = Vec::new();

    if text.len() <= max_chunk {
        chunks.push(DocumentChunk {
            doc_id: doc_id.to_string(),
            doc_path: doc_path.to_string(),
            doc_name: doc_name.to_string(),
            chunk_index: 0,
            text: text.to_string(),
            char_start: 0,
            char_end: text.len(),
            source: source.to_string(),
            embedding: None,
        });
        return chunks;
    }

    let mut start = 0;
    let mut idx = 0;
    while start < text.len() {
        let end = (start + max_chunk).min(text.len());
        // 尽量在段落/句子边界分割
        let mut cut = end;
        if end < text.len() {
            // 从后往前找换行符或句号
            let slice = &text[start..end];
            if let Some(pos) = slice.rfind('\n') {
                cut = start + pos + 1;
            } else if let Some(pos) = slice.rfind('。') {
                cut = start + pos + 1;
            } else if let Some(pos) = slice.rfind('.') {
                cut = start + pos + 1;
            }
        }

        let chunk_text = &text[start..cut];
        chunks.push(DocumentChunk {
            doc_id: doc_id.to_string(),
            doc_path: doc_path.to_string(),
            doc_name: doc_name.to_string(),
            chunk_index: idx,
            text: chunk_text.to_string(),
            char_start: start,
            char_end: cut,
            source: source.to_string(),
            embedding: None,
        });
        idx += 1;

        // 下一个块的起始位置（考虑重叠）
        start = if cut >= text.len() { text.len() } else { (cut as i64 - overlap as i64).max(start as i64 + 1) as usize };
        if start >= text.len() { break; }
    }

    chunks
}

/// 扫描文件夹中的所有文本文件
fn scan_files_in_folder(folder: &str) -> Result<Vec<PathBuf>, String> {
    let dir = Path::new(folder);
    if !dir.exists() {
        return Err(format!("文件夹不存在: {}", folder));
    }
    if !dir.is_dir() {
        return Err(format!("不是文件夹: {}", folder));
    }

    let mut files = Vec::new();
    walk_dir(dir, &mut files)?;
    Ok(files)
}

fn walk_dir(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("读取目录失败: {}", e))? {
        let entry = entry.map_err(|e| format!("读取条目失败: {}", e))?;
        let path = entry.path();
        if path.is_dir() {
            walk_dir(&path, files)?;
        } else if is_supported_file(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_supported_file(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(ext.to_lowercase().as_str(), "txt" | "md" | "markdown" | "json" | "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf" | "log" | "csv" | "xml" | "html" | "htm" | "css" | "js" | "ts" | "py" | "rs" | "java" | "cpp" | "c" | "h" | "hpp"),
        None => false,
    }
}

/// 简化的 BM25 关键词检索
fn bm25_search<'a>(chunks: &'a [DocumentChunk], query: &str, top_k: usize) -> Vec<(usize, f64, &'a DocumentChunk)> {
    if chunks.is_empty() || query.trim().is_empty() {
        return Vec::new();
    }

    let query_terms: Vec<&str> = query.split_whitespace()
        .filter(|t| t.len() > 1)
        .collect();
    if query_terms.is_empty() {
        return Vec::new();
    }

    let n = chunks.len() as f64;
    let mut df: HashMap<&str, f64> = HashMap::new(); // document frequency
    for chunk in chunks {
        let text_lower = chunk.text.to_lowercase();
        let mut seen = std::collections::HashSet::new();
        for term in &query_terms {
            if text_lower.contains(term) && seen.insert(term) {
                *df.entry(term).or_insert(0.0) += 1.0;
            }
        }
    }

    const K1: f64 = 1.5;
    const B: f64 = 0.75;
    let avg_len: f64 = chunks.iter().map(|c| c.text.len() as f64).sum::<f64>() / n.max(1.0);

    let mut scored: Vec<(usize, f64)> = Vec::new();
    for (i, chunk) in chunks.iter().enumerate() {
        let text_lower = chunk.text.to_lowercase();
        let len = chunk.text.len() as f64;
        let mut score = 0.0;

        for term in &query_terms {
            let tf = text_lower.matches(term).count() as f64;
            let idf = ((n - df.get(term).unwrap_or(&0.0) + 0.5) / (df.get(term).unwrap_or(&0.0) + 0.5)).ln() + 1.0;
            if tf > 0.0 {
                score += idf * (tf * (K1 + 1.0)) / (tf + K1 * (1.0 - B + B * len / avg_len.max(1.0)));
            }
        }
        if score > 0.0 {
            scored.push((i, score));
        }
    }

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);

    scored.into_iter().map(|(i, s)| (i, s, &chunks[i])).collect()
}

/// 索引单个文件
pub fn index_file(path: &Path, source: &str) -> Result<Vec<DocumentChunk>, String> {
    let doc_id = format!("{}", path.display());
    let doc_name = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let doc_path = path.to_string_lossy().to_string();

    let content = parse_text_file(path)?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }

    let chunks = chunk_text(&content, &doc_id, &doc_path, &doc_name, source);
    Ok(chunks)
}

/// 检索知识库
/// 构建知识库上下文（插入到系统提示词前）
pub fn build_kb_context(results: &[serde_json::Value]) -> String {
    if results.is_empty() {
        return String::new();
    }

    let mut ctx = String::from("\n\n以下是从知识库中检索到的相关内容，请参考它们来回答用户的问题：\n");
    let mut seen_docs: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut refs: Vec<&str> = Vec::new();

    for (i, r) in results.iter().enumerate() {
        let text = r["text"].as_str().unwrap_or("");
        let doc_name = r["doc_name"].as_str().unwrap_or("unknown");
        if seen_docs.insert(doc_name) {
            refs.push(doc_name);
        }
        ctx.push_str(&format!("[参考{}/{}] {}\n\n", i + 1, results.len(), text));
    }

    if !refs.is_empty() {
        ctx.push_str(&format!("参考来源：{}\n", refs.join("、")));
    }

    ctx
}

/// 余弦相似度
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { return 0.0; }
    (dot / (norm_a * norm_b)) as f64
}

/// 嵌入向量检索（当嵌入可用时使用语义搜索）
fn embedding_search<'a>(chunks: &'a [DocumentChunk], query_embedding: &[f32], top_k: usize) -> Vec<(usize, f64, &'a DocumentChunk)> {
    let mut scored: Vec<(usize, f64)> = Vec::new();
    for (i, chunk) in chunks.iter().enumerate() {
        if let Some(ref emb) = chunk.embedding {
            let sim = cosine_similarity(query_embedding, emb);
            if sim > 0.0 {
                scored.push((i, sim));
            }
        }
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);
    scored.into_iter().map(|(i, s)| (i, s, &chunks[i])).collect()
}

/// 从嵌入服务器获取文本的嵌入向量
pub fn get_embedding_from_server(text: &str) -> Result<Vec<f32>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
    let resp = client.post("http://127.0.0.1:21314/embedding")
        .json(&serde_json::json!({"content": text}))
        .send()
        .map_err(|e| format!("请求嵌入服务失败: {}", e))?;
    let body: serde_json::Value = resp.json().map_err(|e| format!("解析嵌入响应失败: {}", e))?;
    let embedding = body["embedding"].as_array()
        .ok_or_else(|| "嵌入响应缺少 embedding 字段".to_string())?
        .iter()
        .filter_map(|v| v.as_f64())
        .map(|v| v as f32)
        .collect::<Vec<f32>>();
    if embedding.is_empty() {
        return Err("嵌入向量为空".to_string());
    }
    Ok(embedding)
}

/// 嵌入知识库：为所有无嵌入的分块生成嵌入向量（暂禁用）
#[allow(dead_code)]
pub fn embed_all_chunks(state: &KnowledgeState) -> Result<(usize, Vec<String>), String> {
    let mut chunks = state.chunks.lock().map_err(|e| e.to_string())?;
    let mut embedded = 0usize;
    let mut errors = Vec::new();
    for chunk in chunks.iter_mut() {
        if chunk.embedding.is_some() { continue; }
        match get_embedding_from_server(&chunk.text) {
            Ok(emb) => { chunk.embedding = Some(emb); embedded += 1; }
            Err(e) => errors.push(format!("{}[{}]: {}", chunk.doc_name, chunk.chunk_index, e)),
        }
    }
    if embedded > 0 {
        if let Ok(mut avail) = state.embedding_available.lock() { *avail = true; }
    }
    Ok((embedded, errors))
}

/// 检索知识库（支持嵌入向量检索和 BM25 回退）
pub fn search_knowledge(state: &KnowledgeState, query: &str, top_k: usize) -> Vec<serde_json::Value> {
    let chunks = state.chunks.lock().unwrap();

    // 尝试使用嵌入检索
    let has_embedding = chunks.iter().any(|c| c.embedding.is_some());
    if has_embedding {
        if let Ok(query_emb) = get_embedding_from_server(query) {
            let results = embedding_search(&chunks, &query_emb, top_k);
            if !results.is_empty() {
                return results.into_iter().map(|(_idx, score, chunk)| {
                    serde_json::json!({
                        "score": (score * 100.0).round() / 100.0,
                        "doc_name": chunk.doc_name,
                        "doc_path": chunk.doc_path,
                        "chunk_index": chunk.chunk_index,
                        "text": chunk.text,
                    })
                }).collect();
            }
        }
    }

    // 回退到 BM25 关键词检索
    let results = bm25_search(&chunks, query, top_k);
    results.into_iter().map(|(_idx, score, chunk)| {
        serde_json::json!({
            "score": (score * 100.0).round() / 100.0,
            "doc_name": chunk.doc_name,
            "doc_path": chunk.doc_path,
            "chunk_index": chunk.chunk_index,
            "text": chunk.text,
        })
    }).collect()
}

// ============================================================
// Tauri 命令
// ============================================================
#[tauri::command]
pub fn kb_add_folder(folder: String, state: tauri::State<'_, KnowledgeState>) -> Result<String, String> {
    let path = Path::new(&folder);
    if !path.exists() || !path.is_dir() {
        return Err(format!("文件夹不存在或不是目录: {}", folder));
    }

    let mut folders = state.folders.lock().map_err(|e| e.to_string())?;
    if folders.contains(&folder) {
        return Ok("文件夹已存在".to_string());
    }
    folders.push(folder.clone());

    // 重新索引所有文件夹
    let folders_copy = folders.clone();
    drop(folders);
    let all_chunks = reindex_all_from_folders(&folders_copy);
    let mut chunks = state.chunks.lock().map_err(|e| e.to_string())?;
    *chunks = all_chunks;
    let chunk_count = chunks.len();

    Ok(format!("已添加文件夹: {} (共 {} 个分块)", folder, chunk_count))
}

#[tauri::command]
pub fn kb_list_folders(state: tauri::State<'_, KnowledgeState>) -> Result<Vec<String>, String> {
    let folders = state.folders.lock().map_err(|e| e.to_string())?;
    Ok(folders.clone())
}

#[tauri::command]
pub fn kb_remove_folder(folder: String, state: tauri::State<'_, KnowledgeState>) -> Result<String, String> {
    let mut folders = state.folders.lock().map_err(|e| e.to_string())?;
    let pos = folders.iter().position(|f| *f == folder)
        .ok_or_else(|| "文件夹未找到".to_string())?;
    folders.remove(pos);
    let folders_copy = folders.clone();
    drop(folders);

    // 重新索引剩余文件夹
    let mut all_chunks = Vec::new();
    for f in &folders_copy {
        if let Ok(files) = scan_files_in_folder(f) {
            for file in &files {
                if let Ok(chunks) = index_file(file, "folder") {
                    all_chunks.extend(chunks);
                }
            }
        }
    }
    let mut chunks = state.chunks.lock().map_err(|e| e.to_string())?;
    *chunks = all_chunks;

    Ok(format!("已移除文件夹: {}", folder))
}

fn reindex_all_from_folders(folders: &[String]) -> Vec<DocumentChunk> {
    let mut all = Vec::new();
    for f in folders {
        if let Ok(files) = scan_files_in_folder(f) {
            for file in &files {
                if let Ok(chunks) = index_file(file, "folder") {
                    all.extend(chunks);
                }
            }
        }
    }
    all
}

#[tauri::command]
pub fn kb_index_file(file_path: String, state: tauri::State<'_, KnowledgeState>) -> Result<String, String> {
    let path = Path::new(&file_path);
    if !path.exists() || !path.is_file() {
        return Err(format!("文件不存在: {}", file_path));
    }
    if !is_supported_file(path) {
        return Err(format!("不支持的文件类型: {}", file_path));
    }

    let chunks = index_file(path, "temp")?;
    let mut all_chunks = state.chunks.lock().map_err(|e| e.to_string())?;
    let before = all_chunks.len();
    all_chunks.extend(chunks);
    let added = all_chunks.len() - before;

    Ok(format!("已索引文件: {} ({} 个分块)", file_path, added))
}

#[tauri::command]
pub fn kb_clear_temp(state: tauri::State<'_, KnowledgeState>) -> Result<String, String> {
    let mut chunks = state.chunks.lock().map_err(|e| e.to_string())?;
    let before = chunks.len();
    chunks.retain(|c| c.source != "temp");
    let removed = before - chunks.len();
    Ok(format!("已清除 {} 个临时分块", removed))
}

#[tauri::command]
pub fn kb_search(query: String, top_k: Option<usize>, state: tauri::State<'_, KnowledgeState>) -> Result<Vec<serde_json::Value>, String> {
    let k = top_k.unwrap_or(5);
    Ok(search_knowledge(&state, &query, k))
}

#[tauri::command]
pub fn kb_get_context(query: String, top_k: Option<usize>, state: tauri::State<'_, KnowledgeState>) -> Result<String, String> {
    let enabled = state.enabled.lock().map_err(|e| e.to_string())?;
    if !*enabled {
        return Ok(String::new());
    }
    drop(enabled);

    let k = top_k.unwrap_or(3);
    let results = search_knowledge(&state, &query, k);
    Ok(build_kb_context(&results))
}

#[tauri::command]
pub fn kb_toggle(enabled: bool, state: tauri::State<'_, KnowledgeState>) -> Result<String, String> {
    let mut e = state.enabled.lock().map_err(|e| e.to_string())?;
    *e = enabled;
    Ok(if enabled { "知识库已启用" } else { "知识库已禁用" }.to_string())
}

#[tauri::command]
pub fn kb_status(state: tauri::State<'_, KnowledgeState>) -> Result<serde_json::Value, String> {
    let chunks = state.chunks.lock().map_err(|e| e.to_string())?;
    let folders = state.folders.lock().map_err(|e| e.to_string())?;
    let enabled = state.enabled.lock().map_err(|e| e.to_string())?;
    let embedding_avail = state.embedding_available.lock().map_err(|e| e.to_string())?;
    let embedded_count = chunks.iter().filter(|c| c.embedding.is_some()).count();
    Ok(serde_json::json!({
        "enabled": *enabled,
        "folders": folders.len(),
        "chunks": chunks.len(),
        "folders_list": folders.clone(),
        "embedding_available": *embedding_avail,
        "embedded_chunks": embedded_count,
    }))
}

#[tauri::command]
pub fn kb_embed_all(_state: tauri::State<'_, KnowledgeState>) -> Result<String, String> {
    Ok("嵌入功能已禁用（0.5.x 版本启用）".to_string())
}

/// 读取文件内容（供工具调用）—— 安全读取，限制大小
#[tauri::command]
pub fn kb_read_file(path: String) -> Result<String, String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("文件不存在: {}", path));
    }
    // 限制读取文件大小（最大 10MB，文本文件 10MB ≈ 200 万汉字）
    let metadata = std::fs::metadata(p).map_err(|e| format!("读取文件元数据失败: {}", e))?;
    if metadata.len() > 10_485_760 {
        return Err("文件超过 1MB 限制，无法读取".to_string());
    }
    // 检查文件类型（仅允许文本文件）
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    let text_exts = ["txt", "md", "json", "py", "rs", "js", "ts", "java", "cpp", "c", "h",
                     "css", "html", "xml", "yaml", "yml", "toml", "ini", "cfg", "conf",
                     "sh", "bat", "ps1", "csv", "log", "env", "gitignore"];
    if !text_exts.contains(&ext) {
        return Err(format!("不支持的文件类型: .{}", ext));
    }
    let content = std::fs::read_to_string(p)
        .map_err(|e| format!("读取文件失败: {}", e))?;
    // 截断到 8000 字符
    if content.len() > 8000 {
        let truncated = &content[..8000];
        return Ok(format!("{}\n\n... [文件过长，仅显示前 8000 字符，共 {} 字符]", truncated, content.len()));
    }
    Ok(content)
}
