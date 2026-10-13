//! Loopback-only draft relay fixture for manual qp/browser interoperability tests.
//! Not a self-hosted relay implementation: no persistence, room API or recovery.
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use futures_util::SinkExt;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

const VERSION: &str = "1-draft.1";
#[derive(Clone)]
struct Shared {
    room: Arc<Mutex<Room>>,
    audience_token: String,
    presenter_token: String,
}
struct Member {
    sender: mpsc::Sender<Value>,
    results: HashMap<String, (Value, Instant)>,
    requests: HashMap<String, Value>,
}
struct Pending {
    connection: String,
    request: String,
    presenter_epoch: String,
    deadline: Instant,
}
struct Room {
    presenter: Option<mpsc::Sender<Value>>,
    presenter_epoch: u64,
    display_epoch: u64,
    revision: u64,
    paused: bool,
    next: u64,
    members: HashMap<String, Member>,
    pending: HashMap<String, Pending>,
}
impl Room {
    fn state(&self, presenter: bool) -> Value {
        let mut v = json!({"protocol":if presenter {"qp.presenter"} else {"qp.audience"},"version":VERSION,"type":"state","revision":self.revision.to_string(),"available":self.presenter.is_some()});
        if presenter {
            v["display_epoch"] = json!(self.display_epoch.to_string());
            v["audience_connections"] = json!(self.members.len());
        }
        v
    }
    fn changed(&mut self) {
        self.revision += 1;
        if let Some(p) = &self.presenter {
            let _ = p.try_send(self.state(true));
        }
        let state = self.state(false);
        for m in self.members.values() {
            let _ = m.sender.try_send(state.clone());
        }
    }
    fn complete(&mut self, id: &str, status: &str) {
        if let Some(p) = self.pending.remove(id) {
            self.result(&p.connection, &p.request, status);
        }
    }
    fn result(&mut self, connection: &str, request: &str, status: &str) {
        if let Some(member) = self.members.get_mut(connection) {
            let result = json!({"protocol":"qp.audience","version":VERSION,"type":"result","request_id":request,"status":status});
            if member.results.len() < 256 {
                member
                    .results
                    .insert(request.into(), (result.clone(), Instant::now()));
            }
            let _ = member.sender.try_send(result);
        }
    }
    fn lose_presenter(&mut self) {
        self.presenter = None;
        let ids: Vec<_> = self.pending.keys().cloned().collect();
        for id in ids {
            self.complete(&id, "unknown");
        }
        self.changed();
    }
}
fn secret() -> anyhow::Result<String> {
    let mut bytes = [0u8; 43];
    getrandom::fill(&mut bytes)
        .map_err(|e| anyhow::anyhow!("Cannot generate fixture token: {e}"))?;
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
    Ok(bytes
        .iter()
        .map(|b| ALPHABET[(b & 63) as usize] as char)
        .collect())
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let path = std::env::args_os().nth(1).ok_or_else(|| {
        anyhow::anyhow!("Usage: cargo run --example audience-relay-fixture -- <configuration-path>")
    })?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let shared = Shared {
        room: Arc::new(Mutex::new(Room {
            presenter: None,
            presenter_epoch: 0,
            display_epoch: 0,
            revision: 0,
            paused: false,
            next: 0,
            members: HashMap::new(),
            pending: HashMap::new(),
        })),
        audience_token: secret()?,
        presenter_token: secret()?,
    };
    let config = json!({"room_id":"fixture","code":"FIXTURE","join_url":format!("http://{address}/join/fixture#k={}",shared.audience_token),"presenter_url":format!("ws://{address}/ws/presenter"),"presenter_token":shared.presenter_token});
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options.open(&path)?;
    writeln!(file, "{}", serde_json::to_string_pretty(&config)?)?;
    println!("Fixture ready on http://{address}. Set QUICK_PRESENTER_AUDIENCE_RELAY_CONFIG to the configuration path before starting qp. Tokens are stored only in that file.");
    let app = Router::new()
        .route("/join/fixture", get(|| async { Html(PAGE) }))
        .route("/ws/presenter", get(presenter))
        .route("/ws/audience", get(audience))
        .with_state(shared);
    axum::serve(listener, app).await?;
    Ok(())
}
async fn presenter(ws: WebSocketUpgrade, State(s): State<Shared>) -> impl IntoResponse {
    ws.max_message_size(4096)
        .max_frame_size(4096)
        .on_upgrade(move |ws| connection(ws, s, true))
}
async fn audience(ws: WebSocketUpgrade, State(s): State<Shared>) -> impl IntoResponse {
    ws.max_message_size(4096)
        .max_frame_size(4096)
        .on_upgrade(move |ws| connection(ws, s, false))
}
async fn connection(mut ws: WebSocket, s: Shared, presenter: bool) {
    let hello = tokio::time::timeout(Duration::from_secs(5), ws.recv()).await;
    let Ok(Some(Ok(Message::Text(hello)))) = hello else {
        return;
    };
    let Ok(hello) = serde_json::from_str::<Value>(&hello) else {
        return;
    };
    let protocol = if presenter {
        "qp.presenter"
    } else {
        "qp.audience"
    };
    let token = if presenter {
        &s.presenter_token
    } else {
        &s.audience_token
    };
    if hello["protocol"] != protocol
        || hello["version"] != VERSION
        || hello["type"] != "hello"
        || hello["token"] != *token
    {
        return;
    }
    let (sender, mut outgoing) = mpsc::channel::<Value>(32);
    let (id, epoch, welcome) = {
        let mut r = s.room.lock().unwrap();
        r.next += 1;
        let id = format!("c{}", r.next);
        if presenter {
            r.lose_presenter();
            r.presenter_epoch += 1;
            r.presenter = Some(sender);
            (
                id.clone(),
                r.presenter_epoch,
                json!({"protocol":protocol,"version":VERSION,"type":"presenter_welcome","presenter_epoch":r.presenter_epoch.to_string(),"display_epoch":r.display_epoch.to_string(),"audience_connections":r.members.len()}),
            )
        } else {
            r.members.insert(
                id.clone(),
                Member {
                    sender,
                    results: HashMap::new(),
                    requests: HashMap::new(),
                },
            );
            (
                id.clone(),
                r.presenter_epoch,
                json!({"protocol":protocol,"version":VERSION,"type":"welcome","connection_id":id,"capabilities":["reactions","comments"],"available":r.presenter.is_some(),"limits":{"max_message_bytes":4096,"request_cache_ms":30000}}),
            )
        }
    };
    if ws
        .send(Message::Text(welcome.to_string().into()))
        .await
        .is_err()
    {
        cleanup(&s, &id, epoch, presenter);
        return;
    }
    s.room.lock().unwrap().changed();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            value=outgoing.recv()=> {
                let Some(value)=value else {break};
                if !matches!(tokio::time::timeout(Duration::from_secs(2),ws.send(Message::Text(value.to_string().into()))).await, Ok(Ok(()))) {break;}
            }
            _=tick.tick()=> {
                let mut r=s.room.lock().unwrap();
                if presenter && epoch!=r.presenter_epoch {break;}
                let expired:Vec<_>=r.pending.iter().filter(|(_,p)|Instant::now()>=p.deadline).map(|(id,_)|id.clone()).collect();
                for id in expired {r.complete(&id,"unknown");}
            }
            message=ws.recv()=> {
                match message {
                    Some(Ok(Message::Text(text)))=> {
                        let Ok(value)=serde_json::from_str::<Value>(&text) else {break};
                        if value["version"]!=VERSION || value["protocol"]!=protocol {break;}
                        let mut r=s.room.lock().unwrap();
                        if presenter {
                            if epoch!=r.presenter_epoch || value["presenter_epoch"]!=epoch.to_string() {break;}
                            match value["type"].as_str() {
                                Some("display")=> {
                                    let next=value["display_epoch"].as_str().and_then(|v|v.parse::<u64>().ok());
                                    if let (Some(next),Some(paused))=(next,value["paused"].as_bool()) {if next>r.display_epoch {r.display_epoch=next;r.paused=paused;r.changed();}}
                                }
                                Some("receipt")=> {
                                    let Some(event_id)=value["event_id"].as_str() else {break};
                                    let Some(status)=value["status"].as_str().filter(|s|["accepted","busy","stopped"].contains(s)) else {break};
                                    if r.pending.get(event_id).is_some_and(|p|p.presenter_epoch==epoch.to_string()) {r.complete(event_id,status);}
                                }
                                _=>break,
                            }
                        } else {
                            let Some(request)=value["request_id"].as_str().filter(|v|!v.is_empty() && v.len()<=64 && v.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'||b==b'-')) else {break};
                            let member=r.members.get_mut(&id).unwrap();
                            member.results.retain(|_,(_,time)|time.elapsed()<Duration::from_secs(30));
                            let cached_ids:Vec<_>=member.results.keys().cloned().collect();
                            let pending_ids:Vec<_>=r.pending.values().filter(|p|p.connection==id).map(|p|p.request.clone()).collect();
                            let member=r.members.get_mut(&id).unwrap();
                            member.requests.retain(|id,_|cached_ids.contains(id)||pending_ids.contains(id));
                            if let Some(old)=member.requests.get(request) {if old!=&value {break;}}
                            if let Some((cached,_))=member.results.get(request) {let _=member.sender.try_send(cached.clone());continue;}
                            if member.requests.len()>=256 {break;}
                            member.requests.insert(request.into(),value.clone());
                            if r.pending.values().any(|p|p.connection==id && p.request==request) {continue;}
                            if r.presenter.is_none() {r.result(&id,request,"unavailable");continue;}
                            if r.pending.len()>=128 || r.members[&id].results.len()>=256 {r.result(&id,request,"busy");continue;}
                            let payload=match value["type"].as_str() {
                                Some("reaction") if value["kind"].as_str().is_some_and(|k|["thumbs_up","heart","applause","laugh","question"].contains(&k))=>json!({"type":"reaction","kind":value["kind"]}),
                                Some("comment") if value["body"].as_str().is_some_and(|b|b.len()<=800 && b.chars().count()<=200 && !b.trim().is_empty()) && value["handle"].as_str().is_some_and(|h|h.len()<=128 && h.chars().count()<=32)=>json!({"type":"comment","body":value["body"],"handle":value["handle"]}),
                                _=>break,
                            };
                            r.next+=1;let event_id=format!("e{}",r.next);
                            let event=json!({"protocol":"qp.presenter","version":VERSION,"type":"event","event_id":event_id,"connection_id":id,"request_id":request,"presenter_epoch":r.presenter_epoch.to_string(),"display_epoch":r.display_epoch.to_string(),"offer_ttl_ms":2000,"payload":payload});
                            if r.presenter.as_ref().unwrap().try_send(event).is_err() {r.result(&id,request,"busy");continue;}
                            let presenter_epoch=r.presenter_epoch.to_string();
                            r.pending.insert(event_id,Pending{connection:id.clone(),request:request.into(),presenter_epoch,deadline:Instant::now()+Duration::from_secs(3)});
                        }
                    }
                    Some(Ok(Message::Ping(_)))=> {if ws.flush().await.is_err() {break;}}
                    Some(Ok(Message::Pong(_)))=>{}
                    _=>break,
                }
            }
        }
    }
    cleanup(&s, &id, epoch, presenter);
}
fn cleanup(s: &Shared, id: &str, epoch: u64, presenter: bool) {
    let mut r = s.room.lock().unwrap();
    if presenter {
        if epoch == r.presenter_epoch {
            r.lose_presenter();
        }
    } else {
        r.members.remove(id);
        r.pending.retain(|_, p| p.connection != id);
        r.changed();
    }
}
const PAGE: &str = r#"<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>qp Relay Fixture</title>
<style>body{font:18px system-ui;max-width:480px;margin:30px auto;padding:16px}button,input{font:inherit;padding:12px;margin:4px}input{width:90%}</style>
<h1>Audience Live · Test Relay</h1><p>This loopback fixture is for development only.</p><p id="status">Connecting…</p>
<div id="reactions"></div><input id="handle" placeholder="Name" maxlength="32"><input id="body" placeholder="Comment" maxlength="200"><button id="send">Send comment</button>
<script>
const token=new URLSearchParams(location.hash.slice(1)).get('k');history.replaceState(null,'',location.pathname);
const ws=new WebSocket('ws://'+location.host+'/ws/audience');let next=0;
const status=document.getElementById('status');const send=(type,payload)=>{if(ws.readyState===1)ws.send(JSON.stringify({protocol:'qp.audience',version:'1-draft.1',type,request_id:'r'+(++next),...payload}));};
ws.onopen=()=>ws.send(JSON.stringify({protocol:'qp.audience',version:'1-draft.1',type:'hello',token}));
ws.onmessage=e=>{const m=JSON.parse(e.data);status.textContent=m.type==='result'?'Result: '+m.status:m.available?'Connected':'Waiting for presenter';};
ws.onclose=()=>status.textContent='Disconnected. Reload the original join URL to reconnect.';
for(const [kind,label] of [['thumbs_up','👍'],['heart','❤️'],['applause','👏'],['laugh','😂'],['question','❓']]){const b=document.createElement('button');b.textContent=label;b.onclick=()=>send('reaction',{kind});document.getElementById('reactions').append(b);}
document.getElementById('send').onclick=()=>send('comment',{body:document.getElementById('body').value,handle:document.getElementById('handle').value});
</script></html>"#;

#[cfg(test)]
pub(crate) fn start_for_test(
    path: std::path::PathBuf,
) -> (
    tokio::sync::oneshot::Sender<()>,
    std::thread::JoinHandle<()>,
) {
    let (stop, receiver) = tokio::sync::oneshot::channel();
    let (ready, started) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address=listener.local_addr().unwrap();
            let shared=Shared {room:Arc::new(Mutex::new(Room {presenter:None,presenter_epoch:0,display_epoch:0,revision:0,paused:false,next:0,members:HashMap::new(),pending:HashMap::new()})),audience_token:secret().unwrap(),presenter_token:secret().unwrap()};
            let config=json!({"room_id":"fixture","code":"FIXTURE","join_url":format!("http://{address}/join/fixture#k={}",shared.audience_token),"presenter_url":format!("ws://{address}/ws/presenter"),"presenter_token":shared.presenter_token});
            let mut options=std::fs::OpenOptions::new();options.write(true).create_new(true);
            #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt; options.mode(0o600);}
            use std::io::Write;
            let mut file=options.open(path).unwrap();write!(file,"{config}").unwrap();
            let app=Router::new().route("/join/fixture",get(||async {Html(PAGE)})).route("/ws/presenter",get(presenter)).route("/ws/audience",get(audience)).with_state(shared);
            ready.send(()).unwrap();
            tokio::select! { result=axum::serve(listener,app)=>result.unwrap(), _=receiver=>{} }
        });
    });
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    (stop, worker)
}
