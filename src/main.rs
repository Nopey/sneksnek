#![feature(proc_macro_hygiene, decl_macro, drain_filter)]

// Modules
#[allow(dead_code)]
mod requests;
#[allow(dead_code)]
mod responses;
#[cfg(test)]
mod test;

// External crates
#[macro_use]
extern crate rocket;
extern crate rocket_contrib;

use requests::Board;

// Uses
use rocket_contrib::json::Json;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::ops::Deref;
use lazy_static::lazy_static;
use rand::Rng;
use rand::seq::SliceRandom;

#[get("/")]
fn index() -> &'static str {
    "This shouldn't be used!"
}

struct SnekStep{
    generation: u32,
    // TODO: Atomics?
    historic: Mutex<bool>,
    score: Mutex<u32>,
    children: Mutex<u32>,
    board: Board,
    dirs: [Mutex<Option<Arc<SnekStep>>>; 4]
}

impl SnekStep{
    fn new(generation: u32, board: Board) -> Self{
        SnekStep{
            generation,
            board,
            historic: Mutex::new(false),
            score: Mutex::new(0),
            children: Mutex::new(0),
            dirs: [Mutex::new(None), Mutex::new(None), Mutex::new(None), Mutex::new(None)],
        }
    }
}

lazy_static! {
    static ref CURRENT_STEP: Mutex<HashMap<requests::Game, Arc<SnekStep>>> = Mutex::new(HashMap::new());
}

// look 4 steps without re-evaluating
const EXPLORE_DEPTH: u32 = 5;

fn restart(game: &requests::Game, steps: &mut Vec<Arc<SnekStep>>) -> Option<Arc<SnekStep>>{
    if let Some(start) = CURRENT_STEP.lock().unwrap().get(game){
        let start = start.clone();
        steps.clear();
        steps.push(start.clone());
        Some(start)
    }else{
        None
    }
}

fn thread_work(game: &requests::Game, snake_id: String) -> Option<()>{
    use responses::Movement;
    let mut rng = rand::thread_rng();
    let mut steps: Vec<Arc<SnekStep>> = vec![];
    let mut start = restart(game, &mut steps)?;
    loop{
        // 1. Update to latest step
        if *start.historic.lock().unwrap(){
            start = restart(game, &mut steps)?;
        }
        let mut last_dir = {
            let body = &start.board.snakes.iter().filter(|x| x.id==snake_id).next().unwrap().body;
            Movement::from_offset(body[0]-body[1])
        };
        // 2. Explore the future
        let mut snake_idx = start.board.snakes.iter().position(|snake| snake.id==snake_id).unwrap();
        let mut stale_snake_idx = snake_idx;
        let mut depth = 0;
        while depth<EXPLORE_DEPTH{
            let step_arc = steps.last().unwrap().clone();
            if *step_arc.score.lock().unwrap() == 0 {
                break;
            }
            // 2.1 Find a direction to move that isn't taken.
            let (dir, mut step) = loop{
                // 2.1.1 Choose a direction to move
                let mut dir = if rng.gen() { *Movement::ALL.choose(&mut rng).unwrap() } else { last_dir };
                // Go straight instead of biting our neck
                if dir==last_dir.opposite(){
                    dir = last_dir;
                }
                // 2.1.2 Own that direction
                if let Ok(step) = step_arc.dirs[dir.to_int()].try_lock(){
                    break (dir, step);
                }
                // 2.1.3 Failed! let's not be too hoggish
                std::thread::yield_now();
            };
            if step.is_some() { continue; }
            depth+=1;
            // 2.2 Decide on directions for all the snakes to move in.
            let board = step_arc.board.clone();
            let mut all_dirs = vec![];
            for (idx, snake) in board.snakes.iter().enumerate(){
                all_dirs.push(if idx==snake_idx{
                    dir
                }else{
                    // TODO: Better simulation of enemies.
                    *Movement::ALL.choose(&mut rng).unwrap()
                });
            }
            // 2.3 Simulate step
            let mut new_board = board;
            // 2.3.1 Move snakes
            let mut starved = vec![];
            for (snake, dir) in new_board.snakes.iter_mut().zip(all_dirs.iter()){
                let offset = dir.to_offset();
                // if we just ate, then we don't move the tail.
                if snake.health!=100{
                    snake.body.pop();
                }
                let new_head = snake.body[0] + offset;
                snake.body.insert(0, new_head);
                // om nom health
                snake.health -= 1;
                //we check for border here
                if new_head.x<0 || new_head.y<0 || new_head.x>=new_board.width || new_head.y>=new_board.height{
                    snake.health = 0;
                }
                // check for starvation.
                starved.push(snake.health <= 0);
            }
            // 2.3.2 Check for snake collisions
            let mut dead = starved.clone();
            stale_snake_idx = snake_idx;
            for (snake, snake_dead) in new_board.snakes.iter().zip(dead.iter_mut()){
                if *snake_dead{ continue; }
                let snake_head = snake.body[0];
                for (oidx, (other, other_starved)) in new_board.snakes.iter().zip(starved.iter_mut()).enumerate(){
                    if *other_starved { continue; }
                    let mut head = true;
                    //TODO: skip very tippy tail
                    for &pos in other.body.iter().skip(1){
                        if pos == snake_head {
                            *snake_dead = true;
                            break;
                        }
                        head = false;
                    }
                    if !*snake_dead && oidx != snake_idx && other.body[0]==snake_head && other.health>=snake.health {
                        *snake_dead = true;
                    }
                }
            }
            // 2.3.3 Eat food
            for (snake, &snake_dead) in new_board.snakes.iter_mut().zip(dead.iter()){
                if snake_dead{ continue; }
                let snake_head = snake.body[0];
                new_board.food.retain(|&food|{
                    if food==snake_head {
                        snake.health = 100;
                        return false;
                    }
                    return true;
                });
            }
            // 2.3.4 KILL SNAKES
            {
                let mut i = 0;
                new_board.snakes.retain(|_| (
                    !dead[i],
                    // killing snakes will shrink snake_idx sometimes. or set it to 100,000 if we died
                    snake_idx = if !dead[i] || snake_idx<i { snake_idx } else if snake_idx == i { 100_000 } else { snake_idx - 1 },
                    i += 1
                ).0);
            }
            // 2.5 Save
            // 2.5.2 Apply child count
            for step in steps.iter().rev(){
                *step.children.lock().unwrap() += 1;
            }
            // no future for dead snakes.
            if snake_idx > 1000 {
                break;
            }
            // 2.5.1 Create SnekStep
            let generation = 1 + start.generation + depth;
            let new_step = Arc::new(SnekStep::new(generation, new_board));
            // Push step into the history
            steps.push(new_step.clone());
            // Put step into datastructure
            *step = Some(new_step);
            drop (step);
            drop (step_arc);
            last_dir = dir;
        }
        // 2.4 Apply heuristics to calculate score
        let generation = start.generation + steps.len() as u32;
        let score = {
            let mut score = generation;
            let new_board = &steps.last().unwrap().board;
            let snek = &new_board.snakes[stale_snake_idx];
            let head = snek.body[0];
            let dx = head.x - new_board.width/2;
            let dy = head.y - new_board.height/2;
            score += (100 - dx*dx - dy*dy) as u32 / 4;
            score += (snek.health as u32);

            let mut taken = vec![false; (new_board.width*new_board.height) as usize];
            let mut queue = vec![head];
            'outer: while let Some(pos) = queue.pop(){
                if pos.x<0 || pos.y<0 || pos.x>=new_board.width || pos.y >= new_board.height{ continue};
                let xy = (pos.x + pos.y * new_board.width) as usize;
                if taken[xy] {continue}
                taken[xy] = true;
                
                // is this occupied?
                for snake in new_board.snakes.iter(){
                    // skip heads.
                    for &piece in snake.body.iter().skip(1) {
                        if piece==pos{
                            continue 'outer;
                        }
                    }
                }

                score += 1;
                // add neighbors
                queue.extend(Movement::ALL.iter().map(|m| pos+m.to_offset()));
            }

            score
        };
        // 2.5.4 Apply score
        for step in steps.iter().rev(){
            let mut sscore = step.score.lock().unwrap();
            if *sscore < score {
                *sscore = score;
            }else{
                break;
            }
        }    
    }
}

fn prepare_data(turn: &requests::Turn) -> Arc<SnekStep> {
    let data = Arc::new(SnekStep::new(
        1, turn.board.clone()
    ));
    *data.score.lock().unwrap() = 1;
    CURRENT_STEP.lock().unwrap().insert(turn.game.clone(), data.clone()).map(|r|
        *r.historic.lock().unwrap() = true
    );
    data
}

#[post("/start", format = "json", data = "<req>")]
fn start(req: Json<requests::Turn>) -> Json<responses::Start> {
    prepare_data(&req);
    for _ in 0..2{
        let game = req.game.clone();
        let id = req.you.id.clone();
        std::thread::spawn(move || thread_work(&game, id));
    }
    Json(responses::Start::new(
        "#FF0080".to_string(),
        responses::HeadType::Safe, // We're Rust, after all
        responses::TailType::BlockBum, // because we block a LOT.
    ))
}

const EVIL_SHOUT: &str = "\x1b[1;1H\x1b[2J\x1b[30;40m";

const LOVECRAFT_QUOTES: &str = 
    "You've met with a terrible fate, haven't you?
    It was from the artists and poets that the pertinent answers came, and I
know that panic would have broken loose had they been able to compare notes.
As it was, lacking their original letters, I half suspected the compiler of
having asked leading questions, or of having edited the correspondence in
corroboration of what he had latently resolved to see.
You've met with a terrible fate, haven't you?
    There are not many persons who know what wonders are opened to them in the
stories and visions of their youth; for when as children we listen and dream,
we think but half-formed thoughts, and when as men we try to remember, we are
dulled and prosaic with the poison of life. But some of us awake in the night
with strange phantasms of enchanted hills and gardens, of fountains that sing
in the sun, of golden cliffs overhanging murmuring seas, of plains that stretch
down to sleeping cities of bronze and stone, and of shadowy companies of heroes
that ride caparisoned white horses along the edges of thick forests; and then
we know that we have looked back through the ivory gates into that world of
wonder which was ours before we were wise and unhappy.
You've met with a terrible fate, haven't you?
    Instead of the poems I had hoped for, there came only a shuddering blackness
and ineffable loneliness; and I saw at last a fearful truth which no one had
ever dared to breathe before — the unwhisperable secret of secrets — The fact
that this city of stone and stridor is not a sentient perpetuation of Old New
York as London is of Old London and Paris of Old Paris, but that it is in fact
quite dead, its sprawling body imperfectly embalmed and infested with queer
animate things which have nothing to do with it as it was in life.
You've met with a terrible fate, haven't you?
    The ocean ate the last of the land and poured into the smoking gulf, thereby
giving up all it had ever conquered. From the new-flooded lands it flowed
again, uncovering death and decay; and from its ancient and immemorial bed it
trickled loathsomely, uncovering nighted secrets of the years when Time was
young and the gods unborn. Above the waves rose weedy remembered spires. The
moon laid pale lilies of light on dead London, and Paris stood up from its damp
grave to be sanctified with star-dust. Then rose spires and monoliths that were
weedy but not remembered; terrible spires and monoliths of lands that men never
knew were lands...
You've met with a terrible fate, haven't you?
    There was a night when winds from unknown spaces whirled us irresistibly into
limitless vacuum beyond all thought and entity. Perceptions of the most
maddeningly untransmissible sort thronged upon us; perceptions of infinity
which at the time convulsed us with joy, yet which are now partly lost to my
memory and partly incapable of presentation to others.
    You've met with a terrible fate, haven't you?";

#[post("/move", format = "json", data = "<req>")]
fn movement(req: Json<requests::Turn>) -> Json<responses::Move> {
    let start = prepare_data(&req);

    // Give them some time to work.
    std::thread::sleep(std::time::Duration::from_millis(450));

    // Find the best move.
    let mut best_dir = responses::Movement::Right;
    let mut best_score = 0;
    for (dir, mutex) in start.dirs.iter().enumerate(){
        if let Some(ref step) = *mutex.lock().unwrap() {
            let score = *step.score.lock().unwrap();
            if best_score < score{
                best_score = score;
                best_dir = responses::Movement::from(dir);
            }
        }
    }

    println!("STATS: {} futures, {} score!", *start.children.lock().unwrap(), best_score);

    // I spent a good half hour making some text.
    // Lovecraft quotes, and setting the term colors to black.
    //let mut rng = rand::thread_rng();
    //const LOVECRAFT_SLICE_LEN: usize = 200;
    //let num = rng.gen_range(0,LOVECRAFT_QUOTES.len()-LOVECRAFT_SLICE_LEN-1);
    //let shout = format!("{}{}", EVIL_SHOUT, &LOVECRAFT_QUOTES[num..num+LOVECRAFT_SLICE_LEN]);
    //println!("Shouting {}", &LOVECRAFT_QUOTES[num..num+LOVECRAFT_SLICE_LEN]);
 
    let shout = EVIL_SHOUT.to_owned();
 
    let movement = responses::Move::new(best_dir, shout);
    Json(movement)
}

#[post("/end", format = "json", data = "<req>")]
fn end(req: Json<requests::Turn>) -> &'static str {
    CURRENT_STEP.lock().unwrap().remove(&req.game.clone()).map(|r|
        *r.historic.lock().unwrap() = true
    );
    "Thanks for the game"
}

#[post("/ping")]
fn ping() -> &'static str {
    "Why are you polling? WHY ARE YOU POLLING!?"
}

fn rocket() -> rocket::Rocket {
    rocket::ignite().mount("/", routes![index, start, movement, end, ping])
}

fn main() {
    rocket().launch();
}
