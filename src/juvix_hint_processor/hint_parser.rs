use std::str::FromStr;

use super::hint::Hint;
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{alpha1, alphanumeric1, char, multispace0, u64 as parse_u64},
    combinator::{all_consuming, map, recognize},
    multi::many0,
    sequence::{delimited, pair, preceded, tuple},
    IResult,
};

fn parse_usize(input: &str) -> IResult<&str, usize> {
    map(parse_u64, |num: u64| num as usize)(input)
}

fn parse_identifier(input: &str) -> IResult<&str, String> {
    recognize(pair(
        alt((alpha1, tag("_"))),
        many0(alt((alphanumeric1, tag("_")))),
    ))(input)
    .map(|(x, y)| (x, y.to_string()))
}

fn parse_input(input: &str) -> IResult<&str, Hint> {
    map(
        preceded(
            tuple((tag("Input"), multispace0, char('('), multispace0)),
            delimited(
                multispace0,
                parse_identifier,
                tuple((multispace0, char(')'))),
            ),
        ),
        Hint::Input,
    )(input)
}

fn parse_alloc(input: &str) -> IResult<&str, Hint> {
    map(
        preceded(
            tuple((tag("Alloc"), multispace0, char('('))),
            delimited(multispace0, parse_usize, tuple((multispace0, char(')')))),
        ),
        Hint::Alloc,
    )(input)
}

fn parse_hint(input: &str) -> IResult<&str, Hint> {
    all_consuming(delimited(
        multispace0,
        alt((parse_input, parse_alloc)),
        multispace0,
    ))(input)
}

#[derive(Debug)]
pub struct ParseHintError {
    pub message: String,
}

impl FromStr for Hint {
    type Err = ParseHintError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match parse_hint(input) {
            Ok((_, parsed)) => Ok(parsed),
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => Err(ParseHintError {
                message: format!("Error parsing hint {}: {:?}", input, e),
            }),
            Err(nom::Err::Incomplete(needed)) => Err(ParseHintError {
                message: format!(
                    "Error parsing hint - incomplete input: {}. Needed: {:?}",
                    input, needed
                ),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case((r#"Input(variable)"#,
            Hint::Input(String::from("variable"))))]
    #[case((r#"Input(ident1)"#,
            Hint::Input(String::from("ident1"))))]
    #[case((r#"Input(ident1_1)"#,
            Hint::Input(String::from("ident1_1"))))]
    #[case((r#"Input(ident_)"#,
            Hint::Input(String::from("ident_"))))]
    #[case((r#"Input(__ident_)"#,
            Hint::Input(String::from("__ident_"))))]
    #[case((r#"Alloc(123)"#, Hint::Alloc(123)))]
    #[case((r#" Alloc ( 123 ) "#, Hint::Alloc(123)))]
    fn tests_positive(#[case] arg: (&str, Hint)) {
        assert_eq!(arg.0.parse::<Hint>().unwrap(), arg.1)
    }

    #[rstest]
    #[case("nonsense")]
    #[case("Incomplete")]
    #[case("Alloc(34) extra")]
    #[case("Alloc(-1)")]
    #[case("Input(var) extra")]
    #[case("Input(1var)")]
    #[case("Input(var var)")]
    fn tests_negative(#[case] arg: &str) {
        match arg.parse::<Hint>() {
            Ok(_) => assert!(false),
            Err(ParseHintError { message }) => {
                assert!(message.starts_with("Error parsing hint"))
            }
        }
    }
}
