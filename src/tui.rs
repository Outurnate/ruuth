use std::io;
use color_eyre::{eyre::{Result, Context}, owo_colors::OwoColorize};
use zxcvbn::{zxcvbn, feedback::Suggestion};

use crate::user_manager::SetupCode;

pub fn get_password() -> Result<String, io::Error>
{
  let mut password;
  let mut confirm_password;
  loop
  {
    password = rpassword::prompt_password("New password: ")?;
    match zxcvbn(&password, &[])
    {
      Ok(entropy) =>
      {
        let score = entropy.score();
        let meter = match (score, "▮".repeat(score.into()) + &"▯".repeat((4 - score).into()))
        {
          (0, meter) => meter.red().to_string(),
          (1, meter) => meter.yellow().to_string(),
          (2, meter) => meter.bright_yellow().to_string(),
          (_, meter) => meter.green().to_string()
        };
        let crack = entropy.crack_times();
        println!("Strength: {} (online crack time: {}, offline crack time: {})", meter, crack.online_throttling_100_per_hour(), crack.offline_slow_hashing_1e4_per_second());
        if let Some(feedback) = entropy.feedback()
        {
          if let Some(warning) = feedback.warning()
          {
            println!("{}", warning.blink_fast().red().on_black());
          }
          println!("Some suggestions:");
          for suggestion in feedback.suggestions()
          {
            println!("- {}", match suggestion
            {
              Suggestion::UseAFewWordsAvoidCommonPhrases => "Use a few words or common phrases to make up your password",
              Suggestion::NoNeedForSymbolsDigitsOrUppercaseLetters => "Symbols, digits, and uppercase letters are not mandatory",
              Suggestion::AddAnotherWordOrTwo => "Add another word or two",
              Suggestion::CapitalizationDoesntHelpVeryMuch => "Captilization doesn't improve passwords very much",
              Suggestion::AllUppercaseIsAlmostAsEasyToGuessAsAllLowercase => "All uppercase is almost as easy to guess as all lowercase",
              Suggestion::ReversedWordsArentMuchHarderToGuess => "Reversed words aren't much harder to guess than forward words",
              Suggestion::PredictableSubstitutionsDontHelpVeryMuch => "Pr3dictable subst1tutions don't h3lp much",
              Suggestion::UseALongerKeyboardPatternWithMoreTurns => "If you're using a pattern of keys on the keyboard, try changing direction more frequently",
              Suggestion::AvoidRepeatedWordsAndCharacters => "Avoid repeat words or characters",
              Suggestion::AvoidSequences => "Avoid sequences (e.g. 1234)",
              Suggestion::AvoidRecentYears => "Avoid recent years",
              Suggestion::AvoidYearsThatAreAssociatedWithYou => "Avoid years that are significant to you",
              Suggestion::AvoidDatesAndYearsThatAreAssociatedWithYou => "Avoid dates that are associated with you",
            });
          }
        }
        if score < 3
        {
          println!("Password is too weak - try again");
          continue;
        }
      },
      Err(_) => continue
    }
    confirm_password = rpassword::prompt_password("Confirm password: ")?;
    if password != confirm_password
    {
      eprintln!("Passwords do not match!");
    }
    else
    {
      return Ok(password);
    }
  }
}

pub fn maybe_show_qr_code(code: SetupCode, show: bool) -> Result<()>
{
  if show
  {
    println!("{}", code.get_qr_code().wrap_err("failed to generate qr code")?);
  }
  else
  {
    println!("{}", code.get_raw_code());
  }
  Ok(())
}