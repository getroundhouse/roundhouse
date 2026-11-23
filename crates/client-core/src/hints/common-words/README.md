# The words English uses most

`words.txt` holds the 10,000 words English uses most on the web, most common first, one to a line. A word typed in lower case that isn't among them is hinted, as `english.rs` says, since past them the model starts to get a word wrong unhinted.

## Where they're from

They're the first 10,000 lines of `count_1w.txt` from [Peter Norvig's Natural Language Corpus Data](https://norvig.com/ngrams/), without its counts. It ranks the 333,333 words the Google Web Trillion Word Corpus uses most. This makes them again, from the file as it was when they were taken:

```sh
curl -sSfLO https://norvig.com/ngrams/count_1w.txt
echo "51df159fd3de12b20e403c108f526e96dbd723d9cabdd5f17955cdc16059e690  count_1w.txt" | shasum -a 256 -c
head -n 10000 count_1w.txt | cut -f1 > words.txt
```

Norvig's code is under MIT. His page states no license for the counts, which come from a corpus the Linguistic Data Consortium distributes. The owner chose them anyway, over SCOWL, Google Books Ngrams and wordfreq, as [#2400](https://github.com/getroundhouse/roundhouse/issues/2400) says.

## Where the cut is

The speech eval heard all of Tech-Sentences' test split with no hints. Lowercase words up to rank 10,000 came out right 94% to 98% of the time. Past it they came out right 91% of the time, then 84% past 30,000, and 56% for words the counts don't have. The words past 10,000 that the model got wrong were mostly technical, such as `git`, `awk`, `cli` and `codec`, and hints wrote lowercase terms there right about twice as often.

A lower cut hints more everyday words that match nothing said: in each of the CI eval's cases, about three more than a cut of 30,000, and five more than hinting no lowercase words.
