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
    static ref GAME: Mutex<Option<FrontEndGame>> = Mutex::new(None);
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

pub struct FrontEndGame {
    uuid: u128,
    instance: u64,
    game: Game,
    ws: Option<WebSocket>,
    state: GameState
}

impl FrontEndGame {
    pub fn card_played(&mut self, card: &Card, document: &Document) {
        if self.game.cards.contains(&card) {
            let number = card.number;

            //perform checks
            self.game.cards_played.push(card.clone());
            if self.game.cards_played == self.game.cards {
                self.state = GameState::Won;
            } else {
                //TODO: Get rid of copy here.
                if self.game.cards.iter().filter(|x| !self.game.cards_played.contains(&x)).find(|&x| x.number < number) != None {
                    self.state = GameState::Lost;
                }
            }

            //make updates
            match self.state {
                GameState::Lost => {
                    self.close_ws();
                    change_screen(document, "lost");
                },
                GameState::Won => {
                    self.close_ws();
                    change_screen(document, "won");
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

            match ws.send_with_u8_array(&encoded[..]) {
                Ok(_) => console_log!("binary message successfully sent"),
                Err(err) => console_log!("error sending message: {:?}", err),
            }
        }
    }

    pub fn connect_ws(&mut self, uuid: u128, instance: u64) {
        let window = web_sys::window().expect("no global window exists");
        let document = window.document().expect("should have a document window");
    
        //WebSocket Setup.
        let ws = WebSocket::new(&("ws://127.0.0.1:3030/ws/".to_owned() + &uuid.to_string() + "/"  + &instance.to_string() + "/")).expect("expected wss adress");
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
            ws.close().expect("should be able to close ws"); //TODO: Just print we couldn't close maybe?
        }
    }
} 

unsafe impl Send for FrontEndGame {}
unsafe impl Sync for FrontEndGame {}


//TODO: Return error here instead?
async fn get_cards() {
    let mut opts = RequestInit::new();
    opts.method("GET");
    opts.mode(RequestMode::Cors);

    let url = format!("http://127.0.0.1:3030/get-cards");

    if let Ok( request ) = Request::new_with_str_and_init(&url, &opts) {
        if let Ok(_) = request.headers().set("Accept", "application/octet-stream"){


            let window = web_sys::window().unwrap();
            if let Ok( resp_value ) = JsFuture::from(window.fetch_with_request(&request)).await {
                assert!(resp_value.is_instance_of::<Response>());
                let resp: Response = resp_value.dyn_into().unwrap();

                if let Ok( abuf ) = JsFuture::from(resp.array_buffer().expect("js")).await {
                    let array = js_sys::Uint8Array::new(&abuf);

                    let array_u8 = &array.to_vec()[..];

                    let (uuid, instance, game): (u128,u64,Game) = bincode::deserialize(array_u8).unwrap();

                    console_log!("instance: {:?}", instance);

                    console_log!("player cards");
                    console_log!("length of cards: {}",game.cards.len());
                    for card in &game.cards {
                        console_log!("{:?}", card.number);
                    }

                    let mut game_global = GAME.lock().unwrap();

                    *game_global = Some(FrontEndGame{uuid: uuid, instance: instance, game: game, ws: None, state: GameState::Playing});
                } else {
                    console_log!("error getting array");
                }
            } else {
                //TODO: SET ERROR SCREEN!
                console_log!("error getting response");
            }
        } else {
            console_log!("error setting head");
        }
    } else {
        console_log!("error setting init");
    }
}
fn setup_hand(document: &Document, cards: &Vec<Card>, player: Player, hand_id: &str){
    //TODO: might generate and iter later, that'll make this nicer. 
    let computer_hand = document
        .get_element_by_id(hand_id)
        .expect("should have choice_id on the page")
        .dyn_ref::<HtmlElement>()
        .expect("winnings_element should be an HtmlElement")
        .children();

    for (idx,card) in (cards).into_iter().filter(|&card| card.player == player).enumerate() {
        let card_div = computer_hand
            .get_with_index(idx as u32)
            .expect("should be able to create choice button for letter");

        card_div.set_text_content(Some(&card.number.to_string()));
    }
}

fn setup_screen(document: &Document) {
    let game_global = GAME.lock().unwrap();

    if let Some(game) = &*game_global {
        
        //setup computer hand.
        setup_hand(document, &game.game.cards, Player::Computer, "computer-hand");

        setup_hand(document, &game.game.cards, Player::Person, "person-hand");

    } else {
        //TODO: figure out what to do when game is none.
    }
}

fn change_screen(document: &Document, new_screen: &str) {
    document
        .get_element_by_id("game")
        .expect("should have choice_id on the page")
        .dyn_ref::<HtmlElement>()
        .expect("winnings_element should be an HtmlElement")
        .style()
        .set_property("display", "none")
        .expect("should be able to set winnings_element style to visible");

    document
        .get_element_by_id(new_screen)
        .expect("should have choice_id on the page")
        .dyn_ref::<HtmlElement>()
        .expect("winnings_element should be an HtmlElement")
        .style()
        .set_property("display", "flex")
        .expect("should be able to set winnings_element style to visible");
}

fn setup_play_card(document: &Document ) {

    //TODO: weird bug with fast clicks
    let handle_card_play = Closure::<dyn Fn()>::new(
        move || {
            let window = web_sys::window().expect("no global window exists"); //TODO: see if this can be avoided.
            let document = window.document().expect("should have a document window");

            let mut game_global = GAME.lock().unwrap();

            if let Some(game) = &mut *game_global { 
                let card = game.next_card(Player::Person);

                if let Some(card) = card {
                    let idx_of_player = game.next_card_idx(&card, Player::Person);
                    if let Some(idx) = idx_of_player {
                        play_card_actions(&document, (idx, &card), "person-hand");

                        game.card_played(&card, &document);
                    }
                }
            }
        },
    );

    document
        .get_element_by_id("play-card")
        .expect("should have choice_id on the page")
        .dyn_ref::<HtmlElement>()
        .expect("choice_id should be HtmlElement")
        .set_onmousedown(Some(handle_card_play.as_ref().unchecked_ref()));

    handle_card_play.forget();
}

fn setup_start_game(document: &Document ) {

    let handle_start_game = Closure::<dyn Fn()>::new(
        move || {

            let window = web_sys::window().expect("no global window exists"); //TODO: see if this can be avoided.
            let document = window.document().expect("should have a document window");

            let mut game_global = GAME.lock().unwrap();

            if let Some(game) = &mut *game_global { 
                game.connect_ws(game.uuid, game.instance);

                document
                    .get_element_by_id("start-game-container")
                    .expect("should have choice_id on the page")
                    .dyn_ref::<HtmlElement>()
                    .expect("winnings_element should be an HtmlElement")
                    .style()
                    .set_property("display", "none")
                    .expect("should be able to set winnings_element style to visible");

                document
                    .get_element_by_id("play-card-container")
                    .expect("should have choice_id on the page")
                    .dyn_ref::<HtmlElement>()
                    .expect("winnings_element should be an HtmlElement")
                    .style()
                    .set_property("display", "flex")
                    .expect("should be able to set winnings_element style to visible");
            }
        },
    );

    document
        .get_element_by_id("start-game")
        .expect("should have choice_id on the page")
        .dyn_ref::<HtmlElement>()
        .expect("choice_id should be HtmlElement")
        .set_onclick(Some(handle_start_game.as_ref().unchecked_ref()));

    handle_start_game.forget();
}

async fn setup() {
    let window = web_sys::window().expect("no global window exists");
    let document = window.document().expect("should have a document window");

    get_cards().await;

    setup_start_game(&document);

    setup_play_card(&document);

    setup_screen(&document);
}

//TODO: Return Option
fn parse_message(document: &Document, ws_message: MessageEvent) {

    if let Ok(abuf) = ws_message.data().dyn_into::<js_sys::ArrayBuffer>() {
        let array = js_sys::Uint8Array::new(&abuf);
        let array_u8 = &array.to_vec()[..];
        let card: Card = bincode::deserialize(array_u8).unwrap();

        console_log!("computer played: {} ", card.number);

        let mut game_global = GAME.lock().unwrap();
        if let Some(game) = &mut *game_global {
            let idx_of_player = game.next_card_idx(&card, Player::Computer);
            if let Some(idx) = idx_of_player {
                play_card_actions(&document, (idx, &card), "computer-hand");

                game.card_played(&card, &document);
            } 
        } else {
            //TODO: figure out what to do when game is none.
        }

    } else {
        console_log!("Unsupported event message {:?}", ws_message.data());
    }
}

#[wasm_bindgen(start)]
pub async fn main() -> Result<(), JsValue> {
    // Connect to websocket server
    setup().await;

    Ok(())
}

fn play_card_actions(document: &Document, card_played: (usize,&Card), hand: &str) {
    //TODO: update screen with the card played in the play pool and change hand card simultaniously. 
    let computer_hand = document
        .get_element_by_id(hand)
        .expect("should have choice_id on the page")
        .dyn_ref::<HtmlElement>()
        .expect("winnings_element should be an HtmlElement")
        .children();

    let card_played_element = computer_hand
        .get_with_index(card_played.0 as u32)
        .expect("should be able to create choice button for letter");

    let play_pool_element = document
        .get_element_by_id("played-card")
        .expect("should have choice_id on the page");

    //TODO: probably change what happens here but good for now I think. 
    card_played_element
        .dyn_ref::<HtmlElement>()
        .expect("winnings_element should be an HtmlElement")
        .style()
        .set_property("visibility", "hidden")
        .expect("should be able to set winnings_element style to visible");

    //Rather than highlighting, change fill color to that orange, border to some highlighted color(purple?), and text to default color, and the others will just have an orange border and card writing.
    if card_played.1.player == Player::Person{
        let next_card = card_played_element.next_element_sibling();
        if let Some(next_card_element) = next_card {
            next_card_element
                .dyn_ref::<HtmlElement>()
                .expect("winnings_element should be an HtmlElement")
                .set_attribute("class","next-card card")
                .expect("could not set calss");
        }
    }

    play_pool_element
        .dyn_ref::<HtmlElement>()
        .expect("winnings_element should be an HtmlElement")
        .set_inner_html(&card_played.1.number.to_string());
}