#![windows_subsystem = "windows"] // 指定为 Windows GUI 子系统

use anyhow::{Context, Result};
use chrono::Duration;
use cron::Schedule;
use reqwest::blocking::Client;
use reqwest::Url;
use std::fs::File;
use std::io::copy;
use std::path::Path;
use std::str::FromStr;
use std::{env, fs, ptr};
use winapi::um::wingdi::DEVMODEW;
use winapi::um::winuser::{
    EnumDisplaySettingsW, SystemParametersInfoW, SPIF_UPDATEINIFILE, SPI_SETDESKWALLPAPER,
};
use winreg::enums::*;
use winreg::RegKey;

/// 获取屏幕分辨率
fn get_screen_resolution() -> Result<(u32, u32)> {
    const ENUM_CURRENT_SETTINGS: u32 = -1i32 as u32;

    // 创建并初始化一个DEVMODEW结构体，用于存储显示器信息
    let mut dev_mode: DEVMODEW = unsafe { std::mem::zeroed() };
    dev_mode.dmSize = size_of::<DEVMODEW>() as u16;

    // 获取显示器的物理分辨率
    let result = unsafe { EnumDisplaySettingsW(ptr::null(), ENUM_CURRENT_SETTINGS, &mut dev_mode) };

    if result != 0 {
        let width = dev_mode.dmPelsWidth;
        let height = dev_mode.dmPelsHeight;
        Ok((width, height))
    } else {
        Err(anyhow::anyhow!("无法获取屏幕分辨率"))
    }
}

/// 根据分辨率构造图片URL并下载图片
fn download_bing_wallpaper(resolution: (u32, u32)) -> Result<()> {
    // 请求Bing壁纸JSON数据的URL
    let json_url = "https://www.bing.com/HPImageArchive.aspx?format=js&idx=0&n=1&mkt=zh-CN";

    // 创建HTTP客户端
    let client = Client::new();

    // 发起GET请求并解析JSON
    let response = client
        .get(json_url)
        .send()
        .context("无法获取 Bing 壁纸 JSON 数据")?;
    let json_data: serde_json::Value = response.json().context("无法解析 JSON 数据")?;

    // 提取图片相关信息
    if let Some(image) = json_data["images"].get(0) {
        let urlbase = image["urlbase"].as_str().unwrap_or_default();
        let fullstartdate = image["fullstartdate"].as_str().unwrap_or("unknown_date");

        // 构造高分辨率图片URL
        let image_url = format!(
            "https://www.bing.com{}_UHD.jpg&rf=LaDigue_{}x{}.jpg&pid=hp",
            urlbase, resolution.0, resolution.1
        );

        // 获取 %appdata% 路径
        let appdata_dir = env::var("APPDATA").context("无法获取 APPDATA 环境变量")?;
        let images_dir = Path::new(&appdata_dir).join("BingWallpaper").join("Images");
        if !images_dir.exists() {
            fs::create_dir_all(&images_dir).context("无法创建 Images 目录")?;
        }
        let file_name = format!("{}.jpg", fullstartdate);
        let file_path = images_dir.join(&file_name);
        if file_path.exists() {
            println!("图片已存在: {}", file_path.display());
            return Ok(());
        }
        println!("正在下载图片，URL: {}", image_url);
        download_image(&image_url, &file_path)?;
        // 下载图片到本地
        println!("图片已成功下载到: {}", file_path.display());

        set_wallpaper(file_path.to_str().unwrap())?;
    } else {
        println!("无法找到图片信息");
    }

    Ok(())
}

/// 下载图片到本地
fn download_image(url: &str, file_path: &Path) -> Result<()> {
    // 创建HTTP客户端
    let client = Client::new();

    // 发起GET请求
    let mut response = client
        .get(Url::parse(url)?)
        .send()
        .context("无法下载图片")?;

    // 打开文件以写入
    let mut file = File::create(file_path).context("无法创建文件")?;

    // 将响应内容写入文件
    copy(&mut response, &mut file).context("无法将图片写入文件")?;

    Ok(())
}

/// 设置桌面背景
fn set_wallpaper(file_path: &str) -> Result<()> {
    // 检查文件路径是否存在
    if !Path::new(file_path).exists() {
        return Err(anyhow::anyhow!("指定的文件路径不存在"));
    }

    // 将相对路径转换为绝对路径
    let absolute_path = fs::canonicalize(file_path).context("无法将路径转换为绝对路径")?;

    // 将路径转换为 UTF-16 格式以供 Windows API 使用
    let wide_path: Vec<u16> = absolute_path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0)) // 添加空终止符
        .collect();

    // 调用 Windows API 设置桌面背景
    let result = unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            wide_path.as_ptr() as *mut _,
            SPIF_UPDATEINIFILE,
        )
    };

    if result != 0 {
        println!("桌面背景已成功设置为: {}", absolute_path.display());
        Ok(())
    } else {
        Err(anyhow::anyhow!("无法设置桌面背景"))
    }
}

/// 将程序添加到用户的启动项
fn add_to_startup() -> Result<()> {
    // 获取当前程序的路径
    let exe_path = env::current_exe().context("无法获取当前程序的路径")?;
    let exe_path_str = exe_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("无法将程序路径转换为字符串"))?;

    // 定义注册表路径和键值
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            KEY_WRITE,
        )
        .context("无法打开注册表启动项路径")?;

    // 将程序路径写入启动项
    run_key
        .set_value("BingWallpaper", &exe_path_str)
        .context("无法将程序添加到启动项")?;

    println!("程序已成功添加到启动项");
    Ok(())
}

/// 主函数
#[tokio::main]
async fn main() -> Result<()> {
    add_to_startup().context("添加启动项失败")?;

    // let expression = "0 20 * * * *"; // 每小时的第20分钟执行一次
    let expression = "0 */10 * * * *"; // 每隔10分钟执行一次
    let schedule = Schedule::from_str(expression)?;

    // 在程序启动时立即执行一次任务
    if let Err(e) = tokio::task::spawn_blocking(|| run_task()).await? {
        eprintln!("启动时任务执行失败: {}", e);
    }

    loop {
        let now = chrono::Utc::now(); // 使用 chrono::Utc::now() 获取当前 UTC 时间

        let next = match schedule.upcoming(chrono_tz::UTC).next() {
            Some(next_time) => next_time,
            None => {
                eprintln!("无法计算下一次执行时间");
                continue;
            }
        };

        let duration = next.with_timezone(&chrono::Utc) - now;
        println!("下次执行时间: {} 秒后", duration.num_seconds());

        tokio::time::sleep(Duration::to_std(&duration)?).await;

        if let Err(e) = tokio::task::spawn_blocking(|| run_task()).await? {
            eprintln!("任务执行失败: {}", e);
        }
    }
}

fn run_task() -> Result<()> {
    // 获取屏幕分辨率
    let resolution = get_screen_resolution()?;
    println!("屏幕分辨率: {}x{}", resolution.0, resolution.1);

    // 设置桌面背景
    download_bing_wallpaper(resolution)?;

    println!("任务完成：壁纸已成功下载并设置为桌面背景");
    Ok(())
}
