use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, ErrorEvent, HtmlElement, MessageEvent, RequestInit, Request, RequestMode, Response, WebSocket};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use serde::{Deserialize, Serialize};
use lazy_static::lazy_static;
use std::sync::Mutex;

//TODO: lol, better name for this.
pub enum Who {
    Computer,
    Person
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Event<'a> {
    topic: &'a str,
    message: &'a str,
}


#[derive(Serialize, Deserialize, Debug, Copy, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct Card {
    number: u8,
}

//TODO: implement setup of hand and card on screen. Probably dotted line area to show where cards were once played and dotted line area in middle of screen where cards will be played when none have been yet. 
//   and cards with question marks for computers hand. 

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Hand {
    cards: Vec<Card>,
    last_index_played: usize, //TODO: I don't like this. Try a different way later=
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Guesses {
    cards: Vec<Card>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Player {
    hand: Hand,
    guesses: Guesses,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Game {
    level: u8,
    player: Player,
    computer: Player
}

pub struct FrontEndGame {
    uuid: u128,
    id: u64,
    game: Game,
    ws: Option<WebSocket>,
}

unsafe impl Send for FrontEndGame {}
unsafe impl Sync for FrontEndGame {}

//TODO: implement win logic.

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

//TODO: Return error here instead?
async fn get_cards() {
    let mut opts = RequestInit::new();
    opts.method("GET");
    opts.mode(RequestMode::Cors);

    let url = format!("http://127.0.0.1:3030/get-cards");

    if let Ok( request ) = Request::new_with_str_and_init(&url, &opts) {
        if let Ok(_) = request.headers().set("Accept", "application/octet-stream"){


            let window = web_sys::window().unwrap();
            console_log!("HERE 1");
            if let Ok( resp_value ) = JsFuture::from(window.fetch_with_request(&request)).await {
                console_log!("HERE 2");
                assert!(resp_value.is_instance_of::<Response>());
                console_log!("HERE 3");
                let resp: Response = resp_value.dyn_into().unwrap();
                console_log!("HERE 4");

                // Convert this other `Promise` into a rust `Future`.

                
                if let Ok( abuf ) = JsFuture::from(resp.array_buffer().expect("js")).await {
                    console_log!("HERE 5");
                    let array = js_sys::Uint8Array::new(&abuf);

                    console_log!("HERE 6");

                    let array_u8 = &array.to_vec()[..]; //TODO: maybe return byte array to JS, and then parse that on the game side? 

                    console_log!("HERE 7");

                    let (uuid, id, game): (u128,u64,Game) = bincode::deserialize(array_u8).unwrap();

                    console_log!("id: {:?}", id);

                    console_log!("player cards");
                    console_log!("length of hand: {}",game.player.hand.cards.len());
                    for card in &game.player.hand.cards {
                        console_log!("{:?}", card.number);
                    }

                    console_log!("computer cards");
                    console_log!("length of hand: {}",game.computer.hand.cards.len());
                    for card in &game.computer.hand.cards {
                        console_log!("{:?}", card.number);
                    }
                    
                    let mut game_global = GAME.lock().unwrap();

                    *game_global = Some(FrontEndGame{uuid: uuid, id: id, game: game, ws: None});
                } else {
                    console_log!("error getting array");
                }
            } else {
                console_log!("error getting response");
            }
        } else {
            console_log!("error setting head");
        }
    } else {
        console_log!("error setting init");
    }
}

fn setup_screen(document: &Document) {
    let game_global = GAME.lock().unwrap();

    if let Some(game) = &*game_global { 
        
        //TODO: might generate and iter later, that'll make this nicer. 
        for (idx,card) in (&game.game.computer.hand.cards).into_iter().enumerate() {
            let card_id = "card-c".to_owned() + &idx.to_string();
            let card_div = document
                .get_element_by_id(&card_id)
                .expect("should be able to create choice button for letter");

            card_div.set_text_content(Some(&card.number.to_string()));
        }

        for (idx,card) in (&game.game.player.hand.cards).into_iter().enumerate() {
            let card_id = "card-p".to_owned() + &idx.to_string(); //TODO: change to function so there are no mixups!
            let card_div = document
                .get_element_by_id(&card_id)
                .expect("should be able to create choice button for letter");

            card_div.set_text_content(Some(&card.number.to_string()));
        }

    } else {
        //TODO: figure out what to do when game is none.
    }
}

fn setup_play_card(document: &Document ) {

    //TODO: weird bug with fast clicks
    let handle_card_play = Closure::<dyn Fn()>::new(
        move || {
            let window = web_sys::window().expect("no global window exists"); //TODO: see if this can be avoided.
            let document = window.document().expect("should have a document window");

            let mut game_global = GAME.lock().unwrap();

            if let Some(game) = &mut *game_global { 
                if let Some(ws) = &game.ws{
                    console_log!("clicked happened");
                    let next_card_index = if game.game.player.hand.last_index_played+1 < game.game.player.hand.cards.len() { Some(game.game.player.hand.last_index_played+1) } else { None };

                    let card_played = game.game.player.hand.cards[game.game.player.hand.last_index_played];

                    play_card_actions(&document, (game.game.player.hand.last_index_played, card_played.number), next_card_index);

                    

                    let bust = check_bust(&mut game.game, card_played, Who::Person);
                    //gross if blocks here. try ENUM for game status.
                    if bust {
                        //lost screen.
                        if let Some(ws) = &game.ws {
                            ws.close().expect("should be able to close ws");
                        }

                        //display none game screen, show lost screen
                        document
                            .get_element_by_id("game")
                            .expect("should have choice_id on the page")
                            .dyn_ref::<HtmlElement>()
                            .expect("winnings_element should be an HtmlElement")
                            .style()
                            .set_property("display", "none")
                            .expect("should be able to set winnings_element style to visible");

                        document
                            .get_element_by_id("lost")
                            .expect("should have choice_id on the page")
                            .dyn_ref::<HtmlElement>()
                            .expect("winnings_element should be an HtmlElement")
                            .style()
                            .set_property("display", "flex")
                            .expect("should be able to set winnings_element style to visible");
                    } else {

                        let won_game = check_win( &game.game );
                        if won_game {
                            //win screen.
                            if let Some(ws) = &game.ws {
                                ws.close().expect("should be able to close ws");
                            }

                            //display none game screen, show lost screen
                            document
                                .get_element_by_id("game")
                                .expect("should have choice_id on the page")
                                .dyn_ref::<HtmlElement>()
                                .expect("winnings_element should be an HtmlElement")
                                .style()
                                .set_property("display", "none")
                                .expect("should be able to set winnings_element style to visible");

                            document
                                .get_element_by_id("won")
                                .expect("should have choice_id on the page")
                                .dyn_ref::<HtmlElement>()
                                .expect("winnings_element should be an HtmlElement")
                                .style()
                                .set_property("display", "flex")
                                .expect("should be able to set winnings_element style to visible");
                        } else {
                            //TODO: think through if there is an issue with not sending the last card or a card the user played. 
                            send_message(ws,card_played);
                    
                            //TODO: Don't love this, think of change. 
                            game.game.player.hand.last_index_played += 1; 
                        }
                    } 
                    
                    //TODO: the logic to check win also edits the game vector. That should change. When that does this may be needed. As it is, the card is always removed when played. 
                } else {
                    //TODO: handle no ws.
                }
            } else {
                //TODO: figure out what to do when game is none.
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
                game.ws = Some(connect_ws(game.uuid, game.id));
                //TODO: figure out what to do when game is none.

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
        console_log!("message event, received arraybuffer: {:?}", abuf);
        let array = js_sys::Uint8Array::new(&abuf);
        let array_u8 = &array.to_vec()[..];
        let card: Card = bincode::deserialize(array_u8).unwrap();

        console_log!("computer played: {} ", card.number);

        let mut game_global = GAME.lock().unwrap();
        if let Some(game) = &mut *game_global {

            let idx = game.game.computer.hand.last_index_played;
            computer_played(&document, (idx, card.number));
            let bust = check_bust(&mut game.game, card, Who::Computer);
            //gross if blocks here. try ENUM for game status.
            if bust {
                //lost screen.
                if let Some(ws) = &game.ws {
                    ws.close().expect("should be able to close ws");
                }

                //display none game screen, show lost screen
                document
                    .get_element_by_id("game")
                    .expect("should have choice_id on the page")
                    .dyn_ref::<HtmlElement>()
                    .expect("winnings_element should be an HtmlElement")
                    .style()
                    .set_property("display", "none")
                    .expect("should be able to set winnings_element style to visible");

                document
                    .get_element_by_id("lost")
                    .expect("should have choice_id on the page")
                    .dyn_ref::<HtmlElement>()
                    .expect("winnings_element should be an HtmlElement")
                    .style()
                    .set_property("display", "flex")
                    .expect("should be able to set winnings_element style to visible");
            } else {

                let won_game = check_win( &game.game );
                if won_game {
                    //win screen.
                    if let Some(ws) = &game.ws {
                        ws.close().expect("should be able to close ws");
                    }

                    //display none game screen, show lost screen
                    //display none game screen, show lost screen
                    document
                        .get_element_by_id("game")
                        .expect("should have choice_id on the page")
                        .dyn_ref::<HtmlElement>()
                        .expect("winnings_element should be an HtmlElement")
                        .style()
                        .set_property("display", "none")
                        .expect("should be able to set winnings_element style to visible");

                    document
                        .get_element_by_id("won")
                        .expect("should have choice_id on the page")
                        .dyn_ref::<HtmlElement>()
                        .expect("winnings_element should be an HtmlElement")
                        .style()
                        .set_property("display", "flex")
                        .expect("should be able to set winnings_element style to visible");
                } else {
                        game.game.computer.hand.last_index_played += 1;
                }
            } 
        } else {
            //TODO: figure out what to do when game is none.
        }

    } else {
        console_log!("Unsupported event message {:?}", ws_message.data());
    }
}  


//TODO: need actual game id here instead of hardcoded 3.
fn connect_ws(uuid: u128, id: u64) -> WebSocket {
    let window = web_sys::window().expect("no global window exists");
    let document = window.document().expect("should have a document window");

    //TODO: can this ugly string concatination be better? try not to use to_string too. 
    let ws = WebSocket::new(&("ws://127.0.0.1:3030/ws/".to_owned() + &uuid.to_string() + "/"  + &id.to_string() + "/")).expect("expected wss adress");


    console_log!("set ws");
    // For small binary messages, like CBOR, Arraybuffer is more efficient than Blob handling
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);



    let onmessage_callback = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
        parse_message(&document, e);
    });
    
    ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
    // forget the callback to keep it alive
    onmessage_callback.forget();

    let onerror_callback = Closure::<dyn FnMut(_)>::new(move |e: ErrorEvent| {
        console_log!("error event: {:?}", e);
    });
    ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
    onerror_callback.forget();

    ws
}

#[wasm_bindgen(start)]
pub async fn main() -> Result<(), JsValue> {
    // Connect to websocket server
    setup().await;

    Ok(())
}

//TODO: I think we need to know who played this. As well as think if this should be a member function. As well as anything else.
fn check_bust( game: &Game, card: Card, who: Who) -> bool{
    //TODO: check game to see if player sent a card higher than the computer has yet to play. 

    //TODO: this logic can allow a player to play a higher card than the lowest in their hand. Should change to check against a list of cards not played. 

    //TODO: this can be cleaner. easy way if left as is is to return the not polayed and keep all else the saem. 

    //TODO: Idk now i kind of like the idea of popping the first card, like an actual hand of cards would be when a player plays. We will see. 
    let bust = match who {
        Who::Computer => {
            console_log!("computer played");
            let not_played = &game.player.hand.cards[game.player.hand.last_index_played..];
            console_log!("card_played: {}", card.number);
            
            let mut lost = false;
            for card_to_check in not_played {
                if card.number > card_to_check.number {
                    console_log!("card_checked: {}", card_to_check.number);
                    console_log!("Game lost!"); //TODO: handle game lost.
                    lost = true
                }
            }
            lost
        },
        Who::Person => {
            console_log!("player played");
            let not_played = &game.computer.hand.cards[game.computer.hand.last_index_played..];
            console_log!("card_played: {}", card.number);

            let mut lost = false;
            for card_to_check in not_played {
                if card.number > card_to_check.number {
                    console_log!("card_checked: {}", card_to_check.number);
                    console_log!("Game lost!"); //TODO: handle game lost.
                    lost = true
                }
            }
            lost
        },
    };

    bust

}

//TODO: I think we need to know who played this. As well as think if this should be a member function. As well as anything else.
fn check_win( game: &Game ) -> bool {

    if (game.computer.hand.last_index_played == game.computer.hand.cards.len() && game.player.hand.last_index_played == game.player.hand.cards.len() -1) ||  (game.computer.hand.last_index_played == game.computer.hand.cards.len() - 1 && game.player.hand.last_index_played == game.player.hand.cards.len()) {
        true
    } else {
        false
    }

}

fn play_card_actions(document: &Document, card_played: (usize,u8), next_card: Option<usize>) {
    //TODO: update screen with the card played in the play pool and change hand card simultaniously. 
    let card_played_element = document
        .get_element_by_id(&("card-p".to_owned() + &card_played.0.to_string()))
        .expect("should have choice_id on the page");

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
    if let Some(card_idx) = next_card {
        let next_card_element = document
        .get_element_by_id(&("card-p".to_owned() + &card_idx.to_string()))
        .expect("should have choice_id on the page");

        next_card_element
            .dyn_ref::<HtmlElement>()
            .expect("winnings_element should be an HtmlElement")
            .set_attribute("class","next-card card")
            .expect("could not set calss");


    }

    play_pool_element
        .dyn_ref::<HtmlElement>()
        .expect("winnings_element should be an HtmlElement")
        .set_inner_html(&card_played.1.to_string());
}

fn computer_played(document: &Document, card_played: (usize, u8)) {
    //TODO: update screen with the card played in the play pool and change hand card simultaniously. 
    let card_played_element = document
        .get_element_by_id(&("card-c".to_owned() + &card_played.0.to_string()))
        .expect("should have choice_id on the page");

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

    play_pool_element
        .dyn_ref::<HtmlElement>()
        .expect("winnings_element should be an HtmlElement")
        .set_inner_html(&card_played.1.to_string());
}

fn send_message(ws: &WebSocket, played_card: Card) {
    let encoded: Vec<u8> = bincode::serialize(&played_card).unwrap();

    match ws.send_with_u8_array(&encoded[..]) {
        Ok(_) => console_log!("binary message successfully sent"),
        Err(err) => console_log!("error sending message: {:?}", err),
    }
    
}

//IMPORTANT: REALLY ODD ISSUE WITH ONLY 6 cards but 7 being filled and doubling the first number. 