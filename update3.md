# update3 — Module design direction (saved verbatim from the user)

> Reference sketch: `userinput/examplemodules.png`

## The idea (user's words, saved word for word)

all the modules have been square for simplicity, and we've played with different ways to make it not look so stale and repetitive, but the squareness is hard to get rid of. so i want to share with you what i'm thinking about how modules can be done. look at modeuleexamples.png in userinputs/ for some ideas (i made it in paint, don't laugh). starting in top left module, the first two are full rooms, then you get to a t-shaped module (bottom mid) that leads directly into a room at north, further removing the modular feel and making it more customized feel. the bottom right room has an S-shaped open corridor ending in a stairwell, and the top mid and top right rooms both have 3 small rooms each with doors between the rooms. do you understand better now how i want the modules made? also notice that not all doors/openings are centralized on the side. once we have the different walls/roofs/floors that we want, and once we have some rules set up for the connectors (maybe 3 positions on each side), and allow all modules to rotate for 4 times more options, it will be much easier to make like 30 structurally different combinations of this, all with a customizeable number of openings at given places (not all can have openings in all places), then we will have hundreds of possible final modules available, and once we do, the procgen will be easier to guide to create what we want (and more fun).

## Key takeaways (my interpretation — for implementation later)

- **Modules are no longer "one square room = one cell".** A single module can contain
  multiple internal rooms, T/S shapes, corridors, and stairwells, which kills the
  repetitive modular feel.
- **Connector slots are standardized**: ~3 candidate opening positions per side
  (NOT just centered). A given module declares which of those slots *can* be an
  opening; the procgen then chooses how many/which are actually open.
- **All modules rotate (×4)** → 4× the layouts from each authored shape.
- **Goal: ~30 structurally distinct authored modules** → with rotation + variable
  opening sets → hundreds of final module permutations for the procgen to draw from.
- The procgen becomes easier to *steer* (and more fun) once the building blocks are
  this expressive.

### Example modules in the sketch (top-left → bottom-right)
1. Top-left: full room, single off-center connector.
2. Bottom-left: full room, single side connector.
3. Bottom-mid: **T-shaped** module that opens directly into a room to the north.
4. Bottom-right: **S-shaped** open corridor ending in a **stairwell** (vertical link).
5. Top-mid & top-right: each is **3 small rooms** with interior doors between them.

> NOT IMPLEMENTING THE FULL SYSTEM YET — this file is the saved spec to build toward.
