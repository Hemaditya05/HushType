use hushtype_text::{default_terms, is_hallucination, process, AppContext, Dictionary, ProcessOptions};

fn run(raw: &str) -> String {
    run_with(raw, ProcessOptions::default())
}

fn run_with(raw: &str, opts: ProcessOptions) -> String {
    let dict = Dictionary::new(&default_terms());
    process(raw, &opts, &dict)
}

fn ctx(c: AppContext) -> ProcessOptions {
    ProcessOptions { context: c, ..Default::default() }
}

#[test]
fn spec_filler_example() {
    assert_eq!(
        run("uh can you like create a function that um gets the user profile and uh returns their email"),
        "Can you create a function that gets the user profile and returns their email?"
    );
}

#[test]
fn spec_plain_sentence() {
    assert_eq!(
        run("Create a function that fetches the user's profile and returns the email address"),
        "Create a function that fetches the user's profile and returns the email address."
    );
}

#[test]
fn spoken_punctuation_basic() {
    assert_eq!(run("hello comma how are you question mark"), "Hello, how are you?");
    // Whisper tends to add its own punctuation around spoken marks.
    assert_eq!(run("Hello comma, how are you question mark?"), "Hello, how are you?");
    assert_eq!(run("Hello, comma, how are you? Question mark."), "Hello, how are you?");
    assert_eq!(run("period"), ".");
    assert_eq!(run("Period."), ".");
}

#[test]
fn spoken_punctuation_all_marks() {
    assert_eq!(run("wait exclamation mark"), "Wait!");
    assert_eq!(run("note colon buy milk semicolon eggs"), "Note: buy milk; eggs.");
    assert_eq!(run("call me open paren later close paren"), "Call me (later).");
    assert_eq!(run("he said open quote hello close quote"), "He said \"hello\".");
    assert_eq!(run("first line new line second line"), "First line\nSecond line.");
    assert_eq!(run("Thanks. New paragraph. Best regards."), "Thanks.\n\nBest regards.");
    assert_eq!(run("new line install dependencies"), "\nInstall dependencies.");
}

#[test]
fn spoken_punctuation_literal_use() {
    assert_eq!(run("the trial period ends tomorrow"), "The trial period ends tomorrow.");
    assert_eq!(run("add a comma after the name"), "Add a comma after the name.");
    assert_eq!(run("put a question mark there"), "Put a question mark there.");
    assert_eq!(run("a period of time"), "A period of time.");
    assert_eq!(run("that is a great quote"), "That is a great quote.");
    assert_eq!(run("add a new line at the end"), "Add a new line at the end.");
}

#[test]
fn fillers_and_commas() {
    assert_eq!(run("Um, I think we should, uh, ship it."), "I think we should ship it.");
    assert_eq!(run("Yes, um, I agree."), "Yes, I agree.");
    assert_eq!(run("So I was, uh... thinking about it"), "So I was thinking about it.");
    // "like" as a real word must survive.
    assert_eq!(run("I like pizza"), "I like pizza.");
    assert_eq!(run("It looks like the build failed"), "It looks like the build failed.");
    assert_eq!(run("Do you like swimming?"), "Do you like swimming?");
    assert_eq!(run("I would like to create a function"), "I would like to create a function.");
}

#[test]
fn aggressive_cleanup() {
    let opts = ProcessOptions { aggressive: true, ..Default::default() };
    assert_eq!(run_with("So, basically, you know, it's like the best option", opts.clone()), "It's the best option.");
    assert_eq!(run_with("I want to, I want to go home", opts), "I want to go home.");
    // Non-aggressive leaves those alone.
    assert_eq!(run("So, basically, it works"), "So, basically, it works.");
}

#[test]
fn repeated_words() {
    assert_eq!(run("the the function is is broken"), "The function is is broken.");
    assert_eq!(run("I- I think we- we should go"), "I think we should go.");
    assert_eq!(run("can can you check"), "Can you check?");
    assert_eq!(run("I know that that is true"), "I know that that is true.");
}

#[test]
fn capitalization_and_spacing() {
    assert_eq!(run("hello   world .  this is  great"), "Hello world. This is great.");
    assert_eq!(run("i think i'm ready"), "I think I'm ready.");
    assert_eq!(run("use e.g. docker for that"), "Use e.g. Docker for that.");
}

#[test]
fn dictionary_terms() {
    assert_eq!(run("we use type script and post gres q l"), "We use TypeScript and PostgreSQL.");
    assert_eq!(run("deploy it to super base"), "Deploy it to Supabase.");
    assert_eq!(run("push it to get hub"), "Push it to GitHub.");
    assert_eq!(run("open it in vs code"), "Open it in VS Code.");
    assert_eq!(run("we need kubernetis and fast api"), "We need Kubernetes and FastAPI.");
    assert_eq!(run("try faster whisper with open ai"), "Try faster-whisper with OpenAI.");
    // Ordinary words stay ordinary outside of code editors.
    assert_eq!(run("how did she react"), "How did she react?");
    assert_eq!(run("she spoke in a whisper"), "She spoke in a whisper.");
    assert_eq!(run_with("create a react component", ctx(AppContext::Code)), "Create a React component.");
    assert_eq!(run("I love React's hooks"), "I love React's hooks.");
}

#[test]
fn numbers() {
    assert_eq!(run("I need twenty five items"), "I need 25 items.");
    assert_eq!(run("it grew three point five percent"), "It grew 3.5%.");
    assert_eq!(run("one of them"), "One of them.");
    assert_eq!(run("back in nineteen ninety nine"), "Back in 1999.");
    assert_eq!(run("one hundred and five people"), "105 people.");
    assert_eq!(run("one two three"), "One two three.");
}

#[test]
fn questions() {
    assert_eq!(run("what is the status of the build"), "What is the status of the build?");
    assert_eq!(run("what I want is a faster build"), "What I want is a faster build.");
    assert_eq!(run("Is it ready."), "Is it ready?");
    assert_eq!(run("do it now"), "Do it now.");
    assert_eq!(run("have a nice day"), "Have a nice day.");
    assert_eq!(run("so how many users do we have"), "So how many users do we have?");
}

#[test]
fn terminal_context() {
    let o = ctx(AppContext::Terminal);
    assert_eq!(run_with("git status", o.clone()), "git status");
    assert_eq!(run_with("Git status.", o.clone()), "git status");
    assert_eq!(run_with("npm install.", o.clone()), "npm install");
    assert_eq!(run_with("Docker ps", o), "docker ps");
}

#[test]
fn toggles_disable_rules() {
    let off = ProcessOptions {
        remove_fillers: false,
        smart_punctuation: false,
        auto_capitalize: false,
        spoken_punctuation: false,
        ..Default::default()
    };
    assert_eq!(run_with("um hello comma world", off), "um hello comma world");
}

#[test]
fn other_languages_skip_english_rules() {
    let de = ProcessOptions { language: "de".into(), ..Default::default() };
    assert_eq!(run_with("ich habe um zehn uhr zeit", de.clone()), "Ich habe um zehn uhr zeit.");
    let ja = ProcessOptions { language: "ja".into(), ..Default::default() };
    assert_eq!(run_with("こんにちは", ja), "こんにちは");
}

#[test]
fn artifacts_and_hallucinations() {
    assert_eq!(run("[BLANK_AUDIO]"), "");
    assert_eq!(run("(upbeat music) hello there"), "Hello there.");
    assert_eq!(run("call me (later) please"), "Call me (later) please.");
    assert!(is_hallucination("Thank you."));
    assert!(is_hallucination(" you"));
    assert!(!is_hallucination("Thank you for the review."));
}
