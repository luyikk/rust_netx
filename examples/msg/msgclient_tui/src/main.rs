mod controller;
mod interface_server;

use crate::controller::ClientController;
use anyhow::Result;
use cursive::align::HAlign;
use cursive::theme::Effect;
use cursive::traits::Nameable;
use cursive::view::{Resizable, ScrollStrategy};
use cursive::views::{
    Dialog, DummyView, EditView, LinearLayout, NamedView, ScrollView, SelectView, TextView,
};
use cursive::{Cursive, CursiveExt, With};
use interface_server::*;
use log::LevelFilter;
use netxclient::prelude::*;
use once_cell::sync::Lazy;
use packer::LogOn;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

/// NETX CLIENT GLOBAL OBJECT
static CLIENT: Lazy<NetxClientArc<DefaultSessionStore>> = Lazy::new(|| {
    // load server info from option.json
    let server_info =
        serde_json::from_str::<ServerOption>(&std::fs::read_to_string("./option.json").unwrap())
            .unwrap();
    NetXClient::new(server_info, DefaultSessionStore::default())
});

/// message count
static MESSAGE_COUNT: AtomicU32 = AtomicU32::new(0);

/// message send enum
enum MessageSend {
    /// login
    Login(String, oneshot::Sender<String>),
    /// talk
    Talk(String),
    /// get all user
    GetAllUser(oneshot::Sender<Vec<(String, String)>>),
    /// private chat
    PrivateChat(String, String),
    /// ping
    Ping(String, oneshot::Sender<f64>),
}

#[tokio::main]
async fn main() -> Result<()> {
    //create logger
    env_logger::Builder::default()
        .filter_module("cursive", LevelFilter::Info)
        .filter_level(LevelFilter::Debug)
        .init();

    let (msg_sender, msg_receiver) = std::sync::mpsc::channel::<String>();

    //init controller,it will be called by server
    CLIENT
        .init(ClientController::new(Arc::downgrade(&CLIENT), msg_sender))
        .await?;

    // message send channel
    let (msg_send_tx, mut msg_recv_rx) = tokio::sync::mpsc::channel::<MessageSend>(32);

    // init send message loop
    tokio::spawn(async move {
        while let Some(msg) = msg_recv_rx.recv().await {
            let client = impl_ref!(CLIENT=>IServer);
            match msg {
                MessageSend::Login(nickname, tx) => {
                    // send login message
                    match client.login(LogOn { nickname }).await {
                        Ok(res) => {
                            if res.success {
                                if let Err(err) = tx.send("Ok".to_string()) {
                                    log::error!("send login res error:{}", err);
                                }
                            } else if let Err(err) = tx.send(res.msg) {
                                log::error!("send login res error:{}", err);
                            }
                        }
                        Err(err) => {
                            log::error!("login error:{}", err);
                            // send login error
                            if let Err(err) = tx.send(err.to_string()) {
                                log::error!("send login res error:{}", err);
                            }
                        }
                    }
                }
                MessageSend::Talk(msg) => {
                    // send talk message
                    if let Err(err) = client.talk(msg).await {
                        log::error!("talk error:{}", err);
                    }
                }
                MessageSend::GetAllUser(tx) => {
                    // get all user
                    match client.get_users().await {
                        Ok(users) => {
                            let mut users = users
                                .into_iter()
                                .map(|u| (u.nickname.clone(), u.nickname))
                                .collect::<Vec<_>>();
                            users.insert(0, ("GLOBAL CHAT".to_string(), "GLOBAL CHAT".to_string()));
                            if let Err(err) = tx.send(users) {
                                log::error!("send get all user error:{}", err);
                            }
                        }
                        Err(err) => {
                            log::error!("get all user error:{}", err);
                        }
                    }
                }
                MessageSend::PrivateChat(to, message) => {
                    // send private chat
                    if let Err(err) = client.to(to, message).await {
                        log::error!("private chat error:{}", err);
                    }
                }
                MessageSend::Ping(to, tx) => {
                    // send ping
                    match client
                        .ping(to, chrono::Local::now().timestamp_nanos_opt().unwrap())
                        .await
                    {
                        Ok(time) => {
                            if let Err(err) = tx.send(
                                (chrono::Local::now().timestamp_nanos_opt().unwrap() - time) as f64
                                    / 1000000f64,
                            ) {
                                log::error!("send ping error:{}", err);
                            }
                        }
                        Err(err) => {
                            log::error!("ping error:{}", err);
                        }
                    }
                }
            }
        }
    });

    let mut siv = Cursive::default();

    // check is connect to server
    let is_connect = CLIENT.connect_network().await;

    // if connect success
    if is_connect.is_ok() {
        siv.add_layer(
            Dialog::around(
                LinearLayout::vertical()
                    .child(DummyView.fixed_height(1))
                    .child(TextView::new("Enter NickName").h_align(HAlign::Center))
                    .child(EditView::new().with_name("username").fixed_width(20))
                    .child(DummyView.fixed_height(1)),
            )
            .title("Message Chat")
            .button("Ok", move |s| {
                let username = s
                    .call_on_name("username", |view: &mut EditView| view.get_content())
                    .unwrap();

                if username.is_empty() {
                    s.add_layer(Dialog::info("Please enter a nickname !".to_string()));
                } else {
                    // create login wait channel
                    let (wait_login_tx, wait_login_rx) = oneshot::channel::<String>();
                    // send login message
                    msg_send_tx
                        .try_send(MessageSend::Login(username.to_string(), wait_login_tx))
                        .unwrap();

                    // wait login result
                    let login_res = wait_login_rx.recv().unwrap();

                    // if login error
                    if login_res != "Ok" {
                        s.add_layer(Dialog::info(format!("Login Error:{}", login_res)));
                    } else {
                        // login success

                        s.pop_layer();

                        // clone msg_send_tx to move into closure
                        let msg_send_tx = msg_send_tx.clone();
                        let msg_send_private = msg_send_tx.clone();
                        let msg_ping_tx = msg_send_tx.clone();
                        s.add_layer(
                            Dialog::new()
                                .title("Message Chat")
                                .content(
                                    //Instead of using a ListView, we use a ScrollView with a LinearLayout inside.
                                    //This allows us to remove the extra lines from the View
                                    LinearLayout::vertical()
                                        .child(
                                            ScrollView::new(
                                                LinearLayout::vertical()
                                                    .child(DummyView.fixed_height(1))
                                                    //Add in a certain amount of dummy views, to make the new messages appear at the bottom
                                                    .with(|messages| {
                                                        for _ in 0..32 {
                                                            messages.add_child(
                                                                DummyView.fixed_height(1),
                                                            );
                                                        }
                                                    })
                                                    .child(DummyView.fixed_height(1))
                                                    .with_name("messages"),
                                            )
                                            .scroll_strategy(ScrollStrategy::StickToBottom)
                                            .with_name("scroll"),
                                        )
                                        .child(
                                            LinearLayout::horizontal()
                                                .child(TextView::new("Me--->").effect(Effect::Bold))
                                                .child(
                                                    TextView::new("GLOBAL CHAT")
                                                        .effect(Effect::Bold)
                                                        .h_align(HAlign::Left)
                                                        .with_name("to"),
                                                ),
                                        )
                                        .child(EditView::new().with_name("message")),
                                )
                                .h_align(HAlign::Center)
                                .button("Send", move |s| event_send_msg(&msg_send_tx, s))
                                .button("Private Chat", move |s| {
                                    event_private_chat(&msg_send_private, s)
                                })
                                .button("Ping", move |s| event_ping(&msg_ping_tx, s))
                                .button("Quit", |s| {
                                    s.quit();
                                })
                                .fixed_size((80, 40)),
                        );
                    }
                }
            }),
        );

        siv.refresh();
        loop {
            siv.step();
            if !siv.is_running() {
                break;
            }

            let mut needs_refresh = false;
            //Non-blocking channel receiver.
            for m in msg_receiver.try_iter() {
                siv.call_on_name("messages", |messages: &mut LinearLayout| {
                    needs_refresh = true;

                    MESSAGE_COUNT.fetch_add(1, Ordering::SeqCst);
                    messages.add_child(TextView::new(m));
                    if MESSAGE_COUNT.load(Ordering::SeqCst) <= 33 {
                        messages.remove_child(0);
                    }
                });
            }

            if needs_refresh {
                siv.refresh();
            }
        }
    } else {
        siv.add_layer(
            Dialog::around(TextView::new(format!(
                "Connect Server Error:{}",
                is_connect.err().unwrap()
            )))
            .button("Ok", |s| {
                s.quit();
            }),
        );
        siv.run();
    }
    Ok(())
}

/// button send message event
fn event_send_msg(msg_send_tx: &Sender<MessageSend>, s: &mut Cursive) {
    // Get the message from the EditView
    let message = s
        .call_on_name("message", |view: &mut EditView| view.get_content())
        .unwrap();
    // If the message is empty, show an error dialog
    if message.is_empty() {
        s.add_layer(
            Dialog::new()
                .title("Message Chat")
                .content(TextView::new("Please enter a message!!"))
                .button("Okay", |s| {
                    s.pop_layer();
                }),
        )
    } else {
        let to_user = s
            .call_on_name("to", |s: &mut TextView| {
                s.get_content().source().to_string()
            })
            .unwrap();

        if to_user == "GLOBAL CHAT" {
            // Global chat
            //Send the message to the server
            if msg_send_tx
                .try_send(MessageSend::Talk(message.to_string()))
                .is_err()
            {
                // If the message could not be sent, show an error dialog
                s.add_layer(
                    Dialog::new()
                        .title("Message Chat")
                        .content(TextView::new("Error Publishing!"))
                        .button("Okay", |s| {
                            s.pop_layer();
                        }),
                )
            } else {
                //Clear out the EditView.
                s.call_on_name("message", |view: &mut EditView| view.set_content(""))
                    .unwrap();

                s.call_on_name("messages", |messages: &mut LinearLayout| {
                    MESSAGE_COUNT.fetch_add(1, Ordering::SeqCst);
                    messages.add_child(TextView::new(format!("Me:{}", message)));
                    if MESSAGE_COUNT.load(Ordering::SeqCst) <= 33 {
                        messages.remove_child(0);
                    }
                });
                s.call_on_name(
                    "scroll",
                    |view: &mut ScrollView<NamedView<LinearLayout>>| {
                        view.scroll_to_bottom();
                        view.set_scroll_strategy(ScrollStrategy::StickToBottom);
                    },
                );
                s.refresh();
            }
        } else {
            // Private chat
            if msg_send_tx
                .try_send(MessageSend::PrivateChat(
                    to_user.clone(),
                    message.to_string(),
                ))
                .is_err()
            {
                // If the message could not be sent, show an error dialog
                s.add_layer(
                    Dialog::new()
                        .title("Message Chat")
                        .content(TextView::new("Error Publishing!"))
                        .button("Okay", |s| {
                            s.pop_layer();
                        }),
                )
            } else {
                //Clear out the EditView.
                s.call_on_name("message", |view: &mut EditView| view.set_content(""))
                    .unwrap();

                s.call_on_name("messages", |messages: &mut LinearLayout| {
                    MESSAGE_COUNT.fetch_add(1, Ordering::SeqCst);
                    messages.add_child(TextView::new(format!("Me->{}:{}", to_user, message)));
                    if MESSAGE_COUNT.load(Ordering::SeqCst) <= 33 {
                        messages.remove_child(0);
                    }
                });
                s.refresh();
                s.call_on_name(
                    "scroll",
                    |view: &mut ScrollView<NamedView<LinearLayout>>| {
                        view.scroll_to_bottom();
                        view.set_scroll_strategy(ScrollStrategy::StickToBottom);
                    },
                );
                s.refresh();
            }
        }
    }
}

/// button private chat event
fn event_private_chat(msg_send_private: &Sender<MessageSend>, s: &mut Cursive) {
    let (tx, rx) = oneshot::channel::<Vec<(String, String)>>();
    if msg_send_private
        .try_send(MessageSend::GetAllUser(tx))
        .is_err()
    {
        s.add_layer(Dialog::info("Private Chat Error".to_string()));
    } else {
        let users = rx.recv().unwrap();
        s.add_layer(
            Dialog::new()
                .title("Private Chat")
                .content(
                    LinearLayout::vertical()
                        .child(DummyView.fixed_height(1))
                        .child(TextView::new("Select User").h_align(HAlign::Center))
                        .child(
                            SelectView::new()
                                .with_all(users)
                                .h_align(HAlign::Center)
                                .on_submit(|s, user: &String| {
                                    s.call_on_name("to", |s: &mut TextView| {
                                        s.set_content(user.to_string())
                                    });
                                    s.pop_layer();
                                })
                                .with_name("user")
                                .fixed_width(20),
                        )
                        .child(DummyView.fixed_height(1)),
                )
                .button("Exit", |s| {
                    s.pop_layer();
                }),
        )
    }
}

/// button ping event
fn event_ping(msg_ping_tx: &Sender<MessageSend>, s: &mut Cursive) {
    let to_user = s
        .call_on_name("to", |s: &mut TextView| {
            s.get_content().source().to_string()
        })
        .unwrap();
    if to_user == "GLOBAL CHAT" {
        s.add_layer(Dialog::info("Can't ping to global chat".to_string()));
        return;
    }
    let (tx, rx) = oneshot::channel::<f64>();
    if msg_ping_tx
        .try_send(MessageSend::Ping(to_user.clone(), tx))
        .is_err()
    {
        s.add_layer(Dialog::info("Ping Error".to_string()));
    } else {
        let ping = rx.recv().unwrap();
        s.add_layer(Dialog::info(format!("Ping->{to_user}:{} ms", ping)));
    }
}
