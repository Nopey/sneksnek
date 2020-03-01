use serde::Deserialize;

#[derive(Deserialize, PartialEq, Eq, Debug, Clone, Copy)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl std::ops::Add for Point {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Point{
            x: self.x + other.x,
            y: self.y + other.y
        }
    }
}

impl std::ops::Sub for Point {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Point{
            x: self.x - other.x,
            y: self.y - other.y
        }
    }
}
#[derive(Deserialize, PartialEq, Eq, Debug)]
pub struct Turn {
    pub game: Game,
    pub turn: u32,
    pub board: Board,
    pub you: Snake,
}

#[derive(Deserialize, PartialEq, Eq, Debug, Hash, Clone)]
pub struct Game {
    pub id: String,
}

#[derive(Deserialize, PartialEq, Eq, Debug, Clone)]
pub struct Board {
    pub height: i32,
    pub width: i32,
    pub food: Vec<Point>,
    pub snakes: Vec<Snake>,
}

#[derive(Deserialize, PartialEq, Eq, Debug, Clone)]
pub struct Snake {
    pub id: String,
    pub name: String,
    pub health: i32,
    pub body: Vec<Point>,
    // This is an option,
    // because the (unsupported, depricated) snake engine doesn't support it.
    pub shout: Option<String>,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn deserialize_turn() {
        let turn1 = r#"{
            "game": {
                "id": "game-id-string"
            },
            "turn": 4,
            "board": {
                "height": 15,
                "width": 15,
                "food": [
                {
                    "x": 1,
                    "y": 3
                }
                ],
                "snakes": [
                    {
                        "id": "snake-id-string",
                        "name": "Sneky Snek",
                        "health": 90,
                        "body": [
                            {
                                "x": 1,
                                "y": 3
                            }
                        ]
                    }
                ]
            },
            "you": {
                "id": "snake-id-string",
                "name": "Sneky Snek",
                "health": 90,
                "body": [
                {
                    "x": 1,
                    "y": 3
                }
                ]
            }
        }"#;

        let correct: Turn = Turn {
            game: Game {
                id: "game-id-string".to_string(),
            },
            turn: 4,
            board: Board {
                height: 15,
                width: 15,
                food: vec![Point { x: 1, y: 3 }],
                snakes: vec![Snake {
                    id: "snake-id-string".to_string(),
                    name: "Sneky Snek".to_string(),
                    health: 90,
                    body: vec![Point { x: 1, y: 3 }],
                }],
            },
            you: Snake {
                id: "snake-id-string".to_string(),
                name: "Sneky Snek".to_string(),
                health: 90,
                body: vec![Point { x: 1, y: 3 }],
            },
        };

        let result: serde_json::Result<Turn> = serde_json::from_str(turn1);
        match result {
            Err(e) => {
                eprintln!("Returned value is Err: {}", e);
                assert!(false);
            }
            Ok(val) => {
                assert_eq!(correct, val);
            }
        }
    }
}
