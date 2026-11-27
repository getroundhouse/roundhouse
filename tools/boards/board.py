"""Writes the rows of a foundation's board, as `docs/standards/design.md` has one, in Brilliant's Blueprint, for an agent
to send with Brilliant's `edit`.

A board is drawn in the design system's own tokens: text by its kind in `look::Type`, by its typography token, and
frames by their tokens of space, size, corner and colour, nested by indentation. On top of the rows, it draws a board's
parts: a section with its heading and what it's for, the approach's principles, the rail of To decide and the Decided
log, and an open question, a board of its own beside the part's board, with its options side by side.

An option drawn to decide can show a value no token holds yet. The `default` design system refuses one, and the `none`
design system refuses every token, as `docs/reference/brilliant.md` says. So such a value is drawn in a role and given a
ref, and `Rows.values` writes a second fence that sets each ref to its value, sent after the first in the same session,
with `edit`'s `designSystem` set to `none`.

    from board import Rows, t

    rows = Rows('Foundations/Type')
    with rows('al(v,g($spacing.md)) s(fill,hug) f[] "Specimen"'):
        rows.add(t('Body text', 'Body'))
        rows.add(rows.text('A value no token holds', 'Body', 'solid(#e3c0aa)'))
    rows.write('.context/drafts/type.bl.md')
    rows.values('.context/drafts/type-values.bl.md')
"""

EDGE = 'st[($color.edge,w($stroke.width.Edge),pos(i))]'
CONTROL_EDGE = 'st[($color.edge.control_edge,w($stroke.width.Edge),pos(i))]'

# An open question's board, laid out as a part's board is, and how wide what it says and each of its options are.
QUESTION = 'al(v,g($spacing.xl),pad($spacing.xl)) s(hug,hug) f[($color.surface.pane)]'
WORDS = 720
OPTION_WIDTH = 360
# What shows an option, on the sidebar's surface, so it stands apart from the board it's on.
SPECIMEN = 'al(v,g($spacing.sm),pad($spacing.md)) s(fill,hug) f[($color.surface.sidebar)] rd($radius.Control)'


def quoted(words):
    return words.replace('"', '\\"')


def t(words, kind='Body', colour='color.text', name='Words', fill=True, size=None, lines=None):
    """Text in one of `look::Type`'s kinds, by its typography token, in a colour token or a `solid(...)`."""
    family = {'PageTitle': '$font.family.title', 'Code': '$font.family.code', 'InlineCode': '$font.family.code'}.get(
        kind, '$font.family')
    font = {'Emphasis': 'Body', 'Strong': 'Body'}.get(kind, kind)
    s = f' s({size})' if size else (' s(fill,hug)' if fill else '')
    paint = colour if colour.startswith('solid(') else f'${colour}'
    lines = lines or ('CodeFile' if kind == 'Code' else None)
    lh = f',lh($font.lineHeight.{lines})' if lines else ''
    return f't("{quoted(words)}",{family},$font.size.{font}{lh}){s} f[({paint})] $typography.{kind} "{name}"'


def raw(words, family='family', size='Body', weight='regular', lines=None, letters=None, colour='color.text',
        name='Words', fill=False):
    """Text by its font's tokens one by one, with no kind, so the canvas names each token it's set in."""
    lh = f',lh($font.lineHeight.{lines})' if lines else ''
    ls = f',ls($font.letterSpacing.{letters})' if letters else ''
    s = ' s(fill,hug)' if fill else ''
    return (f't("{quoted(words)}",$font.{family},$font.size.{size},$font.weight.{weight}{lh}{ls}){s} '
            f'f[(${colour})] "{name}"')


def bare(paint):
    """Whether `paint` is a value no token holds, such as `#e3c0aa` or `solid(#e3c0aa)`."""
    return paint.startswith(('#', 'solid(#'))


class Rows:
    """A board's rows, for one canvas, each indented one step deeper than the frame it's in."""

    def __init__(self, canvas):
        self.canvas = canvas
        self.rows = ['```bl', f'file("{canvas}")']
        self.depth = 0
        self._values = []

    def add(self, row):
        self.rows.append('  ' * self.depth + row)
        return self

    def __call__(self, row):
        """A frame whose children follow, inside a `with` block."""
        self.add(row)
        return self

    def __enter__(self):
        self.depth += 1

    def __exit__(self, *_):
        self.depth -= 1

    def _ref(self, paint):
        ref = f'value{len(self._values)}'
        self._values.append((ref, paint if paint.startswith('solid(') else f'solid({paint})'))
        return f' #{ref}'

    def text(self, words, kind='Body', colour='color.text', **kw):
        """Text, as `t`, in a colour token or in a value no token holds, which `values` sets."""
        if not bare(colour):
            return t(words, kind, colour, **kw)
        return t(words, kind, 'color.text', **kw) + self._ref(colour)

    def fill(self, paint):
        """A fill, in a colour token or in a value no token holds, which `values` sets: the fill, and the ref to end its
        row with."""
        if not bare(paint):
            return f'f[({paint})]', ''
        return 'f[($color.text)]', self._ref(paint)

    def label(self, words, colour='color.text.faint', fill=False):
        self.add(t(words, 'Detail', colour, name='Name', fill=fill))

    def section(self, title, purpose):
        """A section's frame, with its heading and what it's for, whose children follow at one more depth until
        `end`."""
        self('al(v,g($spacing.md)) s(fill,hug) f[] "' + title + '"')
        self.depth += 1
        # A rule over each section's heading, nearer it than the section before, so each reads as one.
        self.add('r s(fill,1) f[($color.edge)] "Rule"')
        with self('al(v,g($spacing.xs)) s(fill,hug) f[] "Head"'):
            self.add(t(title, 'Heading', name='Heading'))
            self.add(t(purpose, 'Body', 'color.text.muted', name='Purpose'))

    def part(self, title, words):
        """A part of a section, with its own heading, whose children follow until `end`."""
        self('al(v,g($spacing.sm)) s(fill,hug) f[] "' + title + '"')
        self.depth += 1
        with self('al(v,g($spacing.xs)) s(fill,hug) f[] "Head"'):
            self.add(t(title, 'Emphasis', name='Heading'))
            self.add(t(words, 'Body', 'color.text.muted', name='Purpose'))

    def end(self):
        self.depth -= 1

    def end_question(self):
        """Ends an open question's board, after its last option."""
        self.depth -= 2

    def approach(self, purpose, principles):
        """The approach's section: each principle's headline beside why."""
        self.section('The approach', purpose)
        with self('al(v) s(fill,hug) f[] "Principles"'):
            for i, (headline, why) in enumerate(principles):
                if i:
                    self.add('r s(fill,1) f[($color.edge)] "Line"')
                with self('al(h,g($spacing.lg),pad($spacing.md,$spacing.none,$spacing.md,$spacing.none)) s(fill,hug) '
                          'f[] "Principle"'):
                    self.add(t(headline, 'Emphasis', size='240,hug', name='Headline'))
                    self.add(t(why, 'Body', 'color.text.muted', name='Why'))
        self.end()

    def rail(self, decided, open_questions=()):
        """To decide, which lists each open question, `(id, name)`, whose board is beside this one, and the Decided log,
        a line for each question decided: `(id, name, choice, when)`."""
        with self(f'al(v,g($spacing.xl),pad($spacing.lg)) s(320,hug) f[($color.surface.sidebar)] {EDGE} '
                  'rd($radius.Card) "Rail"'):
            with self('al(v,g($spacing.sm)) s(fill,hug) f[] "To decide"'):
                self.add(t('To decide', 'Heading', name='Heading'))
                if not open_questions:
                    self.add(t("Nothing is open. What's next is on #2724.", 'Body', 'color.text.muted', name='Open'))
                else:
                    self.add(t('Each is a board of its own, beside this one.', 'Body', 'color.text.muted', name='Open'))
                for qid, name in open_questions:
                    self.add(self.listed(qid, name))
            with self('al(v,g($spacing.md)) s(fill,hug) f[] "Decided"'):
                self.add(t('Decided', 'Heading', name='Heading'))
                if not decided:
                    self.add(t('Nothing has been decided here yet.', 'Body', 'color.text.muted', name='None'))
                for qid, name, choice, when in decided:
                    with self(f'al(v,g($spacing.xs)) s(fill,hug) f[] "{qid}"'):
                        self.add(t(f'{qid} · {name}', 'Emphasis', name='Question'))
                        self.add(t(choice, 'Body', 'color.text', name='Choice'))
                        self.add(t(when, 'Detail', 'color.text.faint', name='When'))

    @staticmethod
    def listed(qid, name):
        """An open question's line in To decide, by its ID and name."""
        return t(f'{qid} · {name}', 'Body', name=qid)

    def question(self, qid, title, between, at):
        """An open question's board, by its ID, at `at`, `(x, y)` on the canvas, beside the part's board, with its title
        and what it's between. What they need to compare follows, such as what the part looks like now, then
        `options`."""
        self(f'{QUESTION} p({at[0]},{at[1]}) "{qid} · {title}"')
        self.depth += 1
        self.add(t(f'{qid} · {title}', 'PageTitle', name='Question'))
        self.add(t(between, 'Body', 'color.text.muted', size=f'{WORDS},hug', name='Between'))

    def options(self):
        """The open question's options, side by side, each with `option`, until `end_question`."""
        self('al(h,g($spacing.xl)) s(hug,hug) f[] "Options"')
        self.depth += 1

    def option(self, label, recommended=False, width=OPTION_WIDTH):
        """An option's frame, `width` wide, with its label, and Recommended beside it on the one the agent recommends.
        What shows it follows, until `cost`."""
        self(f'al(v,g($spacing.sm)) s({width},hug) f[] "{label}"')
        self.depth += 1
        with self('al(h,g($spacing.sm),y(c)) s(fill,hug) f[] "Label"'):
            self.add(t(label, 'Emphasis', fill=False, name='Name'))
            if recommended:
                self.add(t('Recommended', 'Detail', 'color.brand.current', fill=False, name='Recommended'))

    def cost(self, words):
        """What the option costs, which ends it."""
        self.add(t(words, 'Detail', 'color.text.muted', name='Cost'))
        self.depth -= 1

    def write(self, path):
        """The rows as one fence, for `edit`, and how many there are."""
        with open(path, 'w') as out:
            out.write('\n'.join(self.rows + ['```']) + '\n')
        return len(self.rows)

    def values(self, path, preview=None):
        """The fence that sets each value no token holds, to send after `write`'s with `edit`'s `designSystem` set to
        `none`, and a look at the element `preview` once it's applied. How many values it sets."""
        rows = ['```bl', f'file("{self.canvas}")']
        rows += [f'#{ref} f[({paint})]' for ref, paint in self._values]
        if preview:
            rows.append(f'preview({preview})')
        with open(path, 'w') as out:
            out.write('\n'.join(rows + ['```']) + '\n')
        return len(self._values)
