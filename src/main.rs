#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
use egui::FontFamily::Proportional;
use egui::FontId;
use egui::TextStyle::*;
use reqwest::{
    self,
    header::{HeaderMap, HeaderValue, AUTHORIZATION},
};
use serde::Deserialize;
use serde_json::json;
use std::fmt::format;
use std::{
    io::Write,
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};
use tokio::runtime::Runtime;

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    // Log to stdout (if you run with `RUST_LOG=debug`).
    tracing_subscriber::fmt::init();

    let rt = Runtime::new().expect("Unable to create Runtime");

    // Enter the runtime so that `tokio::spawn` is available immediately.
    let _enter = rt.enter();

    // Execute the runtime in its own thread.
    // The future doesn't have to do anything. In this example, it just sleeps forever.
    std::thread::spawn(move || {
        rt.block_on(async {
            loop {
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
        })
    });

    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(440.0, 440.0)),
        ..Default::default()
    };
    eframe::run_native(
        "OpenAI ChatGPT 调试小程序",
        options,
        Box::new(|cc| Box::new(MyApp::new(cc))),
    )
}

fn setup_custom_fonts(ctx: &egui::Context) {
    // 参考：https://github.com/emilk/egui/blob/0.17.0/eframe/examples/custom_font.rs

    // 此内容存在 bug，即：运行时闪烁，会从乱码转换到不乱码的状态

    // 从默认字体开始（我们将添加而不是替换它们）。
    let mut fonts = egui::FontDefinitions::default();

    // 安装我自己的字体（也许支持非拉丁字符）。
    // 支持 .ttf 和 .otf 文件。
    fonts.font_data.insert(
        "my_font".to_owned(),
        egui::FontData::from_static(include_bytes!("./../fonts/ZCOOLKuaiLe-Regular.ttf")),
    );

    // 将我的字体放在首位（最高优先级）用于比例文本：
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "my_font".to_owned());

    // 将我的字体作为等宽字体的最后后备：
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("my_font".to_owned());

    // 告诉 egui 使用这些字体
    // 新字体将在下一帧开始时激活。
    // https://docs.rs/egui/latest/egui/struct.Context.html#method.set_fonts
    ctx.set_fonts(fonts);

    // 获取之前的显示样式
    let mut style = (*ctx.style()).clone();

    // 设置字体大小 text_styles
    style.text_styles = [
        (Heading, FontId::new(30.0, Proportional)),
        (Name("Heading2".into()), FontId::new(25.0, Proportional)),
        (Name("Context".into()), FontId::new(23.0, Proportional)),
        (Body, FontId::new(18.0, Proportional)),
        (Monospace, FontId::new(14.0, Proportional)),
        (Button, FontId::new(14.0, Proportional)),
        (Small, FontId::new(10.0, Proportional)),
    ]
    .into();

    // Mutate global style with above changes
    ctx.set_style(style);
}

#[derive(Debug)]
struct Msg {
    k: u32,
    v: String,
}
impl Msg {
    fn new(k: u32, v: String) -> Self {
        Self { k: k, v: v }
    }
}

#[derive(Debug)]
struct MyApp {
    // Sender/Receiver for async notifications.
    tx: Sender<Msg>,
    rx: Receiver<Msg>,

    token: String,
    proxy: String,
    q: String,
    a: String,

    used: String,
}

impl MyApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();

        let mut token = "请输入您的 token".to_string();
        let mut proxy = "http://127.0.0.1:7893".to_string();
        let filename = "./openai.key";
        if let Ok(data) = std::fs::read_to_string(filename) {
            let lines: Vec<String> = data.split('\n').map(|s| s.to_owned()).collect();
            let mut index = 0;
            for line in lines {
                if index == 1 {
                    if line.len() > 1 {
                        proxy = line.clone();
                    }
                }
                if index == 0 {
                    if line.len() > 1 {
                        token = line.clone();
                    }
                }
                index += 1;
            }
        }

        setup_custom_fonts(&cc.egui_ctx);

        Self {
            tx,
            rx,
            token: token,
            proxy: proxy,
            q: "你好，请说出你的问题。".to_owned(),
            a: "准备中...".to_owned(),
            used: "".to_owned(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Completion {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    text: String,
    index: u32,
    logprobs: Option<u32>,
    finish_reason: String,
}

fn send_req(token: String, q: String, p: String, tx: Sender<Msg>, ctx: egui::Context) {
    let json = json!({
        "model": "text-davinci-003",
        "prompt": &q,
        "max_tokens": 2048,
        "temperature": 0
    });
    println!("json: {:?}", json);

    let mut headers = HeaderMap::new();
    // headers.insert(COOKIE, HeaderValue::from_str("key=value").unwrap());
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&("Bearer ".to_string() + &token)).unwrap(),
    );
    // headers.insert("X-My-Custom-Header", HeaderValue::from_str("foo").unwrap());
    println!("headers: {:?}", headers);

    let client: reqwest::Client;
    if p.to_lowercase().starts_with("http") || p.to_lowercase().starts_with("sock") {
        let proxy = reqwest::Proxy::http(p).unwrap();
        client = reqwest::Client::builder().proxy(proxy).build().unwrap();
    } else {
        client = reqwest::Client::builder().build().unwrap();
    }

    let response = client
        .post("https://api.openai.com/v1/completions")
        .headers(headers)
        .json(&json)
        .send();

    tokio::spawn(async move {
        match response.await {
            Ok(res) => {
                println!("res: {:?}", res);
                if res.status() != 200 {
                    println!("error code: {:?}", res.status());
                    // self.a = res.status().to_string();
                    let _ = tx.send(Msg::new(0, res.status().to_string()));
                    ctx.request_repaint();
                    return;
                }

                let res1 = res
                    .json::<Completion>()
                    .await
                    .expect("Failed to parse JSON");
                if !res1.choices.is_empty() && res1.choices.len() > 0 {
                    println!("res1: {:?}", res1);

                    let _ = tx.send(Msg::new(0, res1.choices[0].text.trim().to_string()));
                    ctx.request_repaint();
                }
            }
            Err(err) => {
                // handle error case
                println!("Error: {}", err);
            }
        }
    });
}

#[derive(Debug, Deserialize)]
pub struct CreditSummary {
    object: String,
    total_granted: f32,
    total_used: f32,
    total_available: f32,
}

fn query_grants(token: String, p: String, tx: Sender<Msg>, ctx: egui::Context) {
    let mut headers = HeaderMap::new();
    // headers.insert(COOKIE, HeaderValue::from_str("key=value").unwrap());
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&("Bearer ".to_string() + &token)).unwrap(),
    );
    // headers.insert("X-My-Custom-Header", HeaderValue::from_str("foo").unwrap());
    println!("headers: {:?}", headers);

    let client: reqwest::Client;
    if p.to_lowercase().starts_with("http") || p.to_lowercase().starts_with("sock") {
        let proxy = reqwest::Proxy::http(p).unwrap();
        client = reqwest::Client::builder().proxy(proxy).build().unwrap();
    } else {
        client = reqwest::Client::builder().build().unwrap();
    }

    let response = client
        .get("https://api.openai.com/dashboard/billing/credit_grants")
        .headers(headers)
        .send();

    tokio::spawn(async move {
        match response.await {
            Ok(res) => {
                println!("res: {:?}", res);
                if res.status() != 200 {
                    println!("error code: {:?}", res.status());
                    // self.a = res.status().to_string();
                    let _ = tx.send(Msg::new(1, res.status().to_string()));
                    ctx.request_repaint();
                    return;
                }

                let res1 = res
                    .json::<CreditSummary>()
                    .await
                    .expect("Failed to parse JSON");
                println!("res1: {:?}", res1);

                let _ = tx.send(Msg::new(
                    1,
                    format!(
                        "{}/{}",
                        res1.total_used.to_string(),
                        res1.total_granted.to_string(),
                    ),
                ));
                ctx.request_repaint();
            }
            Err(err) => {
                // handle error case
                println!("Error: {}", err);
            }
        }
    });
}

impl eframe::App for MyApp {
    fn update(self: &mut MyApp, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Update the counter with the async response.
        if let Ok(res_msg) = self.rx.try_recv() {
            println!("receive: {:?}", res_msg);

            // 收到了提问的回答
            if res_msg.k == 0 {
                self.a = res_msg.v.to_string();

                // 记录到文件中
                let filename = "./openai.txt";
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(filename)
                    .unwrap();
                file.write(format!("Q: {}\n\nA: {}\n\n\n", self.q, self.a).as_bytes())
                    .unwrap();

                // 查询余额
                query_grants(
                    self.token.clone(),
                    self.proxy.clone(),
                    self.tx.clone(),
                    ctx.clone(),
                );
            }

            // 收到了查询余额的结果
            if res_msg.k == 1 {
                self.used = res_msg.v.to_string();
            }
        }

        if self.used == "" {
            self.used = "查询中".to_string();
            // 查询余额
            query_grants(
                self.token.clone(),
                self.proxy.clone(),
                self.tx.clone(),
                ctx.clone(),
            );
        }

        // 中央面板
        egui::CentralPanel::default().show(ctx, |ui| {
            // 显示大文本
            ui.heading("OpenAI ChatGPT 调试小程序");

            // 使用水平布局启动 ui
            ui.horizontal(|ui| {
                // 显示一些文字
                let label = ui.label("token: ");
                // 不允许换行符，按下回车键将导致失去焦点
                let text = ui
                    .text_edit_singleline(&mut self.token)
                    .labelled_by(label.id);
                if text.changed() {
                    println!("token changed: {:?}", self.token);
                    // 有变更，记录到文本里
                    std::fs::write("./openai.key", format!("{}\n{}\n", self.token, self.proxy))
                        .unwrap();
                }
            });

            // 使用水平布局启动 ui
            ui.horizontal(|ui| {
                let label = ui.label("proxy: ");
                let text = ui
                    .text_edit_singleline(&mut self.proxy)
                    .labelled_by(label.id);
                if text.changed() {
                    println!("proxy changed: {:?}", self.proxy);
                    // 有变更，记录到文本里
                    std::fs::write("./openai.key", format!("{}\n{}\n", self.token, self.proxy))
                        .unwrap();
                }
            });

            // 使用水平布局启动 ui
            ui.horizontal(|ui| {
                let label = ui.label("Q: ");
                ui.text_edit_multiline(&mut self.q).labelled_by(label.id);

                // 按钮
                if ui.button("提交").clicked() {
                    self.a = "提交中...".to_owned();

                    send_req(
                        self.token.clone(),
                        self.q.clone(),
                        self.proxy.clone(),
                        self.tx.clone(),
                        ctx.clone(),
                    );
                }
            });

            // 使用水平布局启动 ui
            ui.horizontal(|ui| {
                let label = ui.label("A: ");

                // 构建 UI 节点
                ui.add_sized([380.0, 250.0], egui::TextEdit::multiline(&mut self.a))
                    .labelled_by(label.id);
            });

            // 显示一些文字
            ui.label(format!("已用/总额: {}", self.used));
        });
    }
}
