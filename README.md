# [boolco.dev](https://boolco.dev)
The code for my (joke) website.

Even though I haven't taken the content seriously, I actually put in a lot of attention to detail - you see, when you share your website with programmers, you're going to get a lot of pen testers whether you like it or not :)

Feel free to poke around and try to mess with it, I dare you!

You will also notice that there is minimal JavaScript - this is on purpose, as I wanted to see how far I could go with as little bloat as possible.

## Building
Added this section since building is non-trivial.

First you must generate a "dummy database" using `cargo prepare` so that sqlx static checking will work.  
Then build regularly with `cargo build --release`. Since this is intended to run on a linux machine, I've added my own shortcut `cargo dev build` which will cross compile with the best options.