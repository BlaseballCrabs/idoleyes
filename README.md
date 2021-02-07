# 99 Recs, Mage of Light
(aka idol-recs, idolizer, idoleyes)

## Standard Heuristics
All of these heuristics are calculated, and the ones that did not result in a player are skipped.

* Best by SO/9: This uses the current-season SO/9 to calculate the pitcher likely to score the most strikeouts.
* Best by ruthlessness: This uses ruthlessness as a proxy for SO/9.
* Best by (SO/9)/(SO/AB): This uses the current-season SO/9 and SO/AB to calculate the pitcher likely to score the most strikeouts against the opposing team.

## Joke Heuristics
One of these heuristics is randomly picked and calculated. This is repeated until one results in a player.

* Best by Bestness: This choosees a player based on the percentage of their name name that is the string "Best."
* Best Best by Stars: This chooses the player with the most pitching stars, limited to names containing the string "Best."
* Against Lift: This chooses a pitcher based on the number of teams named "Tokyo Lift" that the pitcher is against.
* Worst by (-SO/9)(SO/AB): This is the inverse of best by (SO/9)/(SO/AB).
* Best by idolization: This chooses the pitcher with the highest position on the idol leaderboard.
* Best by batting stars: This chooses a pitcher based on batting stars.
* Best by name length: This chooses a pitcher based on the number of characters in their name.
* Best by games per game: This chooses the pitcher whose team has the highest (wins + losses)/games for the current season.
* Best by Games per game: This chooses the team with the most pitchers whose names contain the string "Game."
