use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug)]
struct FileInfo {
    path: PathBuf,
    modified: SystemTime,
}

struct FuzzyMatchLog {
    original_a: PathBuf,
    original_b: PathBuf,
    repaired_a: PathBuf,
    repaired_b: PathBuf,
}

fn safe_move(src: &Path, base_dst_folder: &Path) -> std::io::Result<()> {
    if !src.exists() { return Ok(()); }
    let file_name = src.file_name().unwrap();
    let mut target_path = base_dst_folder.join(file_name);
    if !target_path.exists() {
        if !base_dst_folder.exists() { fs::create_dir_all(base_dst_folder)?; }
        return fs::rename(src, target_path);
    }
    let mut version = 1;
    loop {
        let versioned_folder_name = format!("{}{}", base_dst_folder.file_name().unwrap().to_str().unwrap(), version);
        let versioned_folder_path = base_dst_folder.with_file_name(versioned_folder_name);
        target_path = versioned_folder_path.join(file_name);
        if !target_path.exists() {
            if !versioned_folder_path.exists() { fs::create_dir_all(&versioned_folder_path)?; }
            return fs::rename(src, target_path);
        }
        version += 1;
    }
}

fn main() -> std::io::Result<()> {
    // --- 用户配置区域 ---
    let base_dir = Path::new("/Volumes/LY/DCIM/100APPLE");
    let other_dir = Path::new("/Volumes/LY/DCIM/Other");
    let versioned_base_path = Path::new("/Volumes/LY/DCIM/");
    // --------------------

    println!("\n--- 阶段一: 正在整理和修复 Live Photos... ---");
    let mut heic_files = HashMap::new();
    let mut jpg_files = HashMap::new();
    let mut mov_files = HashMap::new();
    let mut all_initial_files = Vec::new();

    for entry in fs::read_dir(base_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            all_initial_files.push(path.clone());
            if path.file_name().unwrap().to_string_lossy().starts_with("._") { continue; }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                let metadata = entry.metadata()?;
                let file_info = FileInfo { path: path.clone(), modified: metadata.modified()? };
                match path.extension().and_then(|s| s.to_str()).map(|s| s.to_lowercase()).as_deref() {
                    Some("heic") => { heic_files.insert(stem.to_string(), file_info); },
                    Some("jpg") | Some("jpeg") => { jpg_files.insert(stem.to_string(), file_info); },
                    Some("mov") => { mov_files.insert(stem.to_string(), file_info); },
                    _ => {}
                }
            }
        }
    }

    let mut files_to_keep = Vec::new();
    let mut processed_pics = Vec::new();
    let mut fuzzy_match_logs = Vec::new();

    // *** 核心修正点 1: 重构配对逻辑为互斥 ***
    for (mov_stem, mov_info) in &mov_files {
        let mut found_pair = false;

        // 优先尝试精确匹配 HEIC
        if let Some(heic_info) = heic_files.get(mov_stem) {
            if heic_info.modified == mov_info.modified && !processed_pics.contains(&heic_info.path) {
                files_to_keep.push(mov_info.path.clone());
                files_to_keep.push(heic_info.path.clone());
                processed_pics.push(heic_info.path.clone());
                found_pair = true;
            }
        }

        // 如果没找到 HEIC 配对，再尝试精确匹配 JPG
        if !found_pair {
            if let Some(jpg_info) = jpg_files.get(mov_stem) {
                if jpg_info.modified == mov_info.modified && !processed_pics.contains(&jpg_info.path) {
                    files_to_keep.push(mov_info.path.clone());
                    files_to_keep.push(jpg_info.path.clone());
                    processed_pics.push(jpg_info.path.clone());
                    found_pair = true;
                }
            }
        }

        // 如果所有精确匹配都失败了，最后才尝试模糊匹配
        if !found_pair {
            if mov_stem.starts_with("IMG_") && mov_stem.len() >= 8 {
                let mov_prefix = &mov_stem[..8];
                let candidate_pics: Vec<_> = heic_files.iter().chain(jpg_files.iter())
                    .filter(|(_, pic_info)| !processed_pics.contains(&pic_info.path) && pic_info.modified == mov_info.modified && pic_info.path.file_stem().unwrap().to_str().unwrap().starts_with(mov_prefix))
                    .collect();
                
                if candidate_pics.len() == 1 {
                    let (pic_stem, pic_info) = candidate_pics[0];
                    let (shorter_path, longer_stem) = if mov_stem.len() > pic_stem.len() {
                        (&pic_info.path, mov_stem)
                    } else {
                        (&mov_info.path, pic_stem)
                    };
                    let new_path = shorter_path.with_file_name(format!("{}.{}", longer_stem, shorter_path.extension().unwrap().to_str().unwrap()));
                    
                    if fs::rename(shorter_path, &new_path).is_ok() {
                        fuzzy_match_logs.push(FuzzyMatchLog {
                            original_a: mov_info.path.clone(), original_b: pic_info.path.clone(),
                            repaired_a: if shorter_path == &mov_info.path { new_path.clone() } else { mov_info.path.clone() },
                            repaired_b: if shorter_path == &pic_info.path { new_path.clone() } else { pic_info.path.clone() },
                        });
                        files_to_keep.push(if shorter_path == &mov_info.path { new_path.clone() } else { mov_info.path.clone() });
                        files_to_keep.push(if shorter_path == &pic_info.path { new_path } else { pic_info.path.clone() });
                        processed_pics.push(pic_info.path.clone());
                    }
                }
            }
        }
    }

    // *** 核心修正点 2: 使用更严格的清理逻辑 ***
    for path in all_initial_files {
        // 只有当文件的完整路径精确存在于保留列表中时，才保留
        if !files_to_keep.contains(&path) {
            safe_move(&path, other_dir)?;
        }
    }
    println!("阶段一完成！");

    // --- 阶段二: 分层迁移与净化 (无变化) ---
    println!("\n--- 阶段二: 正在进行分层迁移与净化... ---");
    let mut files_in_100: Vec<_> = fs::read_dir(base_dir)?.map(|r| r.unwrap().path()).collect();
    files_in_100.sort();

    for path in files_in_100 {
        if !path.is_file() { continue; }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if stem.starts_with("IMG_") && stem.len() > 8 {
                let prefix = &stem[..8];
                let ext = path.extension().unwrap().to_str().unwrap();
                if ext.to_lowercase() != "mov" { continue; }
                let paired_pic_path = path.with_extension("heic");
                let paired_pic_path = if paired_pic_path.exists() { Some(paired_pic_path) } else {
                    let jpg_path = path.with_extension("jpg");
                    if jpg_path.exists() { Some(jpg_path) } else { None }
                };
                if paired_pic_path.is_none() { continue; }
                let paired_pic_path = paired_pic_path.unwrap();
                let mut version = 100;
                loop {
                    let target_dir_name = format!("{}APPLE", version);
                    let target_dir = versioned_base_path.join(target_dir_name);
                    if !target_dir.exists() { fs::create_dir_all(&target_dir)?; }
                    let target_mov_name = format!("{}.mov", prefix);
                    let check_path = target_dir.join(&target_mov_name);
                    if !check_path.exists() {
                        let final_mov_path = target_dir.join(&target_mov_name);
                        if fs::rename(&path, &final_mov_path).is_err() { break; }
                        let pic_ext = paired_pic_path.extension().unwrap().to_str().unwrap();
                        let final_pic_path = target_dir.join(format!("{}.{}", prefix, pic_ext));
                        fs::rename(&paired_pic_path, &final_pic_path)?;
                        break;
                    }
                    version += 1;
                }
            }
        }
    }
    println!("阶段二完成！");

    // --- 最终总结报告 (无变化) ---
    println!("\n--------------------------------------------------");
    println!("             程序运行总结");
    println!("--------------------------------------------------");
    if fuzzy_match_logs.is_empty() {
        println!("本次运行没有发现需要修复的模糊匹配 Live Photo。");
    } else {
        println!("共找到并修复了 {} 对模糊匹配的 Live Photo：\n", fuzzy_match_logs.len());
        for (index, log) in fuzzy_match_logs.iter().enumerate() {
            println!("修复 {}:", index + 1);
            println!("  - 原始文件: {:?}, {:?}", log.original_a.file_name().unwrap(), log.original_b.file_name().unwrap());
            println!("  - 修复后  : {:?}, {:?}", log.repaired_a.file_name().unwrap(), log.repaired_b.file_name().unwrap());
            println!();
        }
    }
    
    let mut total_file_count = 0;
    let mut max_version = 100;
    loop {
        let dir_name_to_check = format!("{}APPLE", max_version);
        let dir_to_check = versioned_base_path.join(dir_name_to_check);
        if !dir_to_check.exists() { break; }
        if let Ok(entries) = fs::read_dir(&dir_to_check) {
            total_file_count += entries.filter_map(Result::ok).filter(|e| e.path().is_file()).count();
        }
        max_version += 1;
    }
    
    let mut other_file_count = 0;
    let mut other_version = 0;
    loop {
        let other_dir_name = if other_version == 0 {
            other_dir.file_name().unwrap().to_str().unwrap().to_string()
        } else {
            format!("{}{}", other_dir.file_name().unwrap().to_str().unwrap(), other_version)
        };
        let other_dir_to_check = other_dir.with_file_name(other_dir_name);
        if !other_dir_to_check.exists() { break; }
        if let Ok(entries) = fs::read_dir(&other_dir_to_check) {
            other_file_count += entries.filter_map(Result::ok).filter(|e| e.path().is_file()).count();
        }
        other_version += 1;
    }

    println!("--------------------------------------------------");
    println!("最终文件统计:");
    println!("  - 所有 Live Photo 文件夹 (100APPLE, 101APPLE, ...) 中的文件总数为: {}", total_file_count);
    println!("  - 所有 Other 文件夹 (Other, Other1, ...) 中的文件总数为: {}", other_file_count);
    println!("\n所有操作已完成！");
    
    Ok(())
}
