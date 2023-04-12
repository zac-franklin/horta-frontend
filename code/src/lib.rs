use wasm_bindgen::prelude::*;
use web_sys::{Document, ErrorEvent, HtmlElement, MessageEvent, RequestInit, Request, RequestMode, Response, WebSocket};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use serde::{Deserialize, Serialize};
use lazy_static::lazy_static;
use std::sync::Mutex;
use std::str;

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

lazy_static! {
    static ref HORTA: Mutex<Option<Horta>> = Mutex::new(None);
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Clone)]
pub enum Player {
    Computer,
    Person
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Clone)]
pub struct Card {
    player: Player,
    number: u8,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Game {
    level: u8,
    cards: Vec<Card>,
    cards_played: Vec<Card>,
}

#[derive(Eq, PartialEq)]
pub enum GameState {
    Playing,
    Won,
    Lost,
}

pub struct Horta {
    uuid: u128,
    instance: u64,
    game: Game,
    ws: Option<WebSocket>,
    state: GameState
}

impl Horta {
    pub fn card_played(&mut self, card: &Card, document: &Document) {
        if self.game.cards.contains(&card) {
            let number = card.number;

            //perform checks
            self.game.cards_played.push(card.clone());
            if self.game.cards_played == self.game.cards {
                self.state = GameState::Won;
            } else {
                if self.game.cards.iter().filter(|x| !self.game.cards_played.contains(&x)).find(|&x| x.number < number) != None {
                    self.state = GameState::Lost;
                }
            }

            //make updates
            match self.state {
                GameState::Lost => {
                    self.close_ws();
                    lost_screen(document, &card.player);
                },
                GameState::Won => {
                    self.close_ws();
                    victory_screen(document);
                },
                GameState::Playing => {
                    if card.player == Player::Person {
                        self.send_card(&card);
                    }
                }
            }
        }
    }

    pub fn next_card(&self, player: Player) -> Option<Card> {
        self.game.cards
            .iter()
            .filter(|&x| !self.game.cards_played.contains(&x))
            .find(|&x| x.player == player)
            .map(|x| x.clone())
    }

    pub fn next_card_idx(&self, card: &Card, player: Player) -> Option<usize> {
        self.game.cards
            .iter()
            .filter(|&x| x.player == player)
            .position(|x| x == card)
    }

    pub fn send_card(&self, card: &Card) {
        if let Some(ws) = &self.ws {
            let encoded: Vec<u8> = bincode::serialize(&card).unwrap();

            if let Err(err) =  ws.send_with_u8_array(&encoded[..]) {
                console_log!("error sending message: {:?}", err);
            }
        }
    }

    pub fn connect_ws(&mut self, uuid: u128, instance: u64) {
        let window = web_sys::window().expect("no global window exists");
        let document = window.document().expect("should have a document window");
    
        //WebSocket Setup.
        let ws = WebSocket::new(&("wss://roonr.com/api/horta/v1/ws/".to_owned() + &uuid.to_string() + "/"  + &instance.to_string() + "/")).expect("expected wss adress");
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
        let onmessage_callback = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
            parse_message(&document, e);
        });
        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();
        let onerror_callback = Closure::<dyn FnMut(_)>::new(move |e: ErrorEvent| {
            console_log!("error event: {:?}", e);
        });
        ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
        onerror_callback.forget();
    
        self.ws = Some(ws);
    }

    fn close_ws(&mut self) {
        if let Some(ws) = &self.ws {
            ws.close().expect("should be able to close ws");
        }
    }
} 

unsafe impl Send for Horta {}
unsafe impl Sync for Horta {}

async fn get_cards() -> Result<&'static str, &'static str> {
    let mut opts = RequestInit::new();
    opts.method("GET");
    opts.mode(RequestMode::Cors);

    let url = format!("https://roonr.com/api/horta/v1/get-cards");

    if let Ok( request ) = Request::new_with_str_and_init(&url, &opts) {
        if let Ok(_) = request.headers().set("Accept", "application/octet-stream"){


            let window = web_sys::window().unwrap();
            if let Ok( resp_value ) = JsFuture::from(window.fetch_with_request(&request)).await {
                assert!(resp_value.is_instance_of::<Response>());
                let resp: Response = resp_value.dyn_into().unwrap();

                if let Ok( abuf ) = JsFuture::from(resp.array_buffer().expect("should have array buffer")).await {
                    let array = js_sys::Uint8Array::new(&abuf);

                    let array_u8 = &array.to_vec()[..];

                    let (uuid, instance, game): (u128,u64,Game) = bincode::deserialize(array_u8).unwrap();

                    let mut horta = HORTA.lock().unwrap();

                    *horta = Some(Horta{uuid: uuid, instance: instance, game: game, ws: None, state: GameState::Playing});

                    Ok("Got Game!")
                } else {
                    Err("error reading array")
                }
            } else {
                Err("error parsing response")
            }
        } else {
            Err("error setting headers")
        }
    } else {
        Err("error setting initializing request")
    }
}

fn setup_hand(document: &Document, cards: &Vec<Card>, player: Player, hand_id: &str){
    let computer_hand = document
        .get_element_by_id(hand_id)
        .expect("should have hand on the page")
        .dyn_ref::<HtmlElement>()
        .expect("hand should be an HtmlElement")
        .children();

    for (idx,card) in (cards).into_iter().filter(|&card| card.player == player).enumerate() {
        let card_div = computer_hand
            .get_with_index(idx as u32)
            .expect("should be able to grab card by index");

        card_div.set_text_content(Some(&card.number.to_string()));
    }
}

fn setup_screen(document: &Document) {
    let horta_lock = HORTA.lock().unwrap();

    if let Some(horta) = &*horta_lock {

        setup_hand(document, &horta.game.cards, Player::Person, "person-hand");

    } 
}

fn lost_screen(document: &Document, player: &Player) {
    document
        .get_element_by_id("err")
        .expect("should have err on the page")
        .dyn_ref::<HtmlElement>()
        .expect("err should be an HtmlElement")
        .style()
        .set_property("display", "none")
        .expect("should be able to set err display to none");

    document
        .get_element_by_id("won")
        .expect("should have won on the page")
        .dyn_ref::<HtmlElement>()
        .expect("won should be an HtmlElement")
        .style()
        .set_property("display", "none")
        .expect("should be able to set won display to none");

    let blurb = match player {
        Player::Computer => {
            "The Computer played a higher card than one you have. Welp, Here is a blurred picure of my dog, Patrick. Bummer you can't see it clearly. It's very cute!"
        },
        Player::Person => {
            "You played a higher card than one the computer has. Welp, Here is a blurred picure of my dog, Patrick. Bummer you can't see it clearly. It's very cute!"
        },
    };

    document
        .get_element_by_id("lost-blurb")
        .expect("should have won on the page")
        .set_inner_html(blurb);

    document
        .get_element_by_id("played-card")
        .expect("should have played-card on the page")
        .dyn_ref::<HtmlElement>()
        .expect("played-card should be an HtmlElement")
        .set_attribute("class","horta-wrong-card")
        .expect("should be able to set class of next card");

    document
        .get_element_by_id("start-game-container")
        .expect("should have start-game-container on the page")
        .dyn_ref::<HtmlElement>()
        .expect("start-game-container should be an HtmlElement")
        .style()
        .set_property("display", "none")
        .expect("should be able to set start-game-container diplay to none");

    document
        .get_element_by_id("play-card-container")
        .expect("should have play-card-container on the page")
        .dyn_ref::<HtmlElement>()
        .expect("play-card-container should be an HtmlElement")
        .style()
        .set_property("display", "none")
        .expect("should be able to set play-card-container display to none");

    document
        .get_element_by_id("lost")
        .expect("should have lost on the page")
        .dyn_ref::<HtmlElement>()
        .expect("lost should be an HtmlElement")
        .style()
        .set_property("display", "flex")
        .expect("should be able to set lost display to flex");
}

fn err_screen(document: &Document) {
    document
        .get_element_by_id("game")
        .expect("should have game on the page")
        .dyn_ref::<HtmlElement>()
        .expect("game should be an HtmlElement")
        .style()
        .set_property("display", "none")
        .expect("should be able to set game display to none");

    document
        .get_element_by_id("lost")
        .expect("should have lost on the page")
        .dyn_ref::<HtmlElement>()
        .expect("lost should be an HtmlElement")
        .style()
        .set_property("display", "none")
        .expect("should be able to set lost display to none");

    document
        .get_element_by_id("won")
        .expect("should have won on the page")
        .dyn_ref::<HtmlElement>()
        .expect("won should be an HtmlElement")
        .style()
        .set_property("display", "none")
        .expect("should be able to set won display to none");

    document
        .get_element_by_id("err")
        .expect("should have err on the page")
        .dyn_ref::<HtmlElement>()
        .expect("err should be an HtmlElement")
        .style()
        .set_property("display", "flex")
        .expect("should be able to set err display to flex");
}

fn victory_screen(document: &Document) {
    document
        .get_element_by_id("game")
        .expect("should have game on the page")
        .dyn_ref::<HtmlElement>()
        .expect("game should be an HtmlElement")
        .style()
        .set_property("display", "none")
        .expect("should be able to set game display to none");

    document
        .get_element_by_id("lost")
        .expect("should have lost on the page")
        .dyn_ref::<HtmlElement>()
        .expect("lost should be an HtmlElement")
        .style()
        .set_property("display", "none")
        .expect("should be able to set lost display to none");

    document
        .get_element_by_id("err")
        .expect("should have err on the page")
        .dyn_ref::<HtmlElement>()
        .expect("err should be an HtmlElement")
        .style()
        .set_property("display", "none")
        .expect("should be able to set err display to none");

    document
        .get_element_by_id("won")
        .expect("should have won on the page")
        .dyn_ref::<HtmlElement>()
        .expect("won should be an HtmlElement")
        .style()
        .set_property("display", "flex")
        .expect("should be able to set won display to flex");
}

fn setup_play_card(document: &Document ) {
    let handle_card_play = Closure::<dyn Fn()>::new(
        move || {
            let window = web_sys::window().expect("no global window exists"); //TODO: see if this can be avoided.
            let document = window.document().expect("should have a document window");

            let mut horta_lock = HORTA.lock().unwrap();

            if let Some(horta) = &mut *horta_lock { 
                let card = horta.next_card(Player::Person);

                if let Some(card) = card {
                    let next_idx = horta.next_card_idx(&card, Player::Person);
                    if let Some(idx) = next_idx {
                        play_card_actions(&document, (idx, &card), "person-hand");

                        horta.card_played(&card, &document);
                    }
                }
            }
        },
    );

    document
        .get_element_by_id("play-card")
        .expect("should have play-card on the page")
        .dyn_ref::<HtmlElement>()
        .expect("cplay-card should be HtmlElement")
        .set_onmousedown(Some(handle_card_play.as_ref().unchecked_ref()));

    handle_card_play.forget();
}

fn setup_start_game(document: &Document ) {
    let handle_start_game = Closure::<dyn Fn()>::new(
        move || {

            let window = web_sys::window().expect("no global window exists"); //TODO: see if this can be avoided.
            let document = window.document().expect("should have a document window");

            let mut horta_lock = HORTA.lock().unwrap();

            if let Some(horta) = &mut *horta_lock { 
                horta.connect_ws(horta.uuid, horta.instance);

                document
                    .get_element_by_id("start-game-container")
                    .expect("should have start-game-container on the page")
                    .dyn_ref::<HtmlElement>()
                    .expect("start-game-container should be an HtmlElement")
                    .style()
                    .set_property("display", "none")
                    .expect("should be able to set start-game-container diplay to none");

                document
                    .get_element_by_id("play-card-container")
                    .expect("should have play-card-container on the page")
                    .dyn_ref::<HtmlElement>()
                    .expect("play-card-container should be an HtmlElement")
                    .style()
                    .set_property("display", "flex")
                    .expect("should be able to set play-card-container display to flex");
            }
        },
    );

    document
        .get_element_by_id("start-game")
        .expect("should have start-game on the page")
        .dyn_ref::<HtmlElement>()
        .expect("start-game should be HtmlElement")
        .set_onclick(Some(handle_start_game.as_ref().unchecked_ref()));

    handle_start_game.forget();
}

async fn setup() {
    let window = web_sys::window().expect("no global window exists");
    let document = window.document().expect("should have a document window");

    match get_cards().await {
        Ok(_) => {
            setup_start_game(&document);

            setup_play_card(&document);

            setup_screen(&document);
        },
        Err(err) => {
            err_screen(&document);
            console_log!("Unsupported event message {:?}", err);
        }
    }

    
}

fn parse_message(document: &Document, ws_message: MessageEvent) {
    if let Ok(abuf) = ws_message.data().dyn_into::<js_sys::ArrayBuffer>() {
        let array = js_sys::Uint8Array::new(&abuf);
        let array_u8 = &array.to_vec()[..];
        if let Ok(card) = bincode::deserialize::<Card>(array_u8){
            let mut horta_lock = HORTA.lock().unwrap();
            if let Some(horta) = &mut *horta_lock {
                let next_idx = horta.next_card_idx(&card, Player::Computer);
                if let Some(idx) = next_idx {
                    play_card_actions(&document, (idx, &card), "computer-hand");

                    horta.card_played(&card, &document);
                } 
            }
        }
    } 
}

#[wasm_bindgen(start)]
pub async fn main() -> Result<(), JsValue> {
    setup().await;

    Ok(())
}

fn play_card_actions(document: &Document, card_played: (usize,&Card), hand: &str) {
    let computer_hand = document
        .get_element_by_id(hand)
        .expect("should have handon the page")
        .dyn_ref::<HtmlElement>()
        .expect("hand should be an HtmlElement")
        .children();

    let card_played_element = computer_hand
        .get_with_index(card_played.0 as u32)
        .expect("should be able to get card of hand wiht index");

    let play_pool_element = document
        .get_element_by_id("played-card")
        .expect("should have played-card on the page");

    card_played_element
        .dyn_ref::<HtmlElement>()
        .expect("card should be an HtmlElement")
        .style()
        .set_property("visibility", "hidden")
        .expect("should be able to set card style to hidden");

    if card_played.1.player == Player::Person{
        let next_card = card_played_element.next_element_sibling();
        if let Some(next_card_element) = next_card {
            next_card_element
                .dyn_ref::<HtmlElement>()
                .expect("next card should be an HtmlElement")
                .set_attribute("class","horta-next-card card")
                .expect("should be able to set class of next card");
        }
    }

    play_pool_element
        .dyn_ref::<HtmlElement>()
        .expect("played-card should be an HtmlElement")
        .set_inner_html(&card_played.1.number.to_string());
}