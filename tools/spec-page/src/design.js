// Leaves only the rows whose name, value or use has what's typed in it, under their headings and captions, and says
// so when there are none.
const find = document.getElementById('find');
const nothing = document.getElementById('nothing');
find.addEventListener('input', () => {
  const query = find.value.trim().toLowerCase();
  document.body.classList.toggle('finding', query !== '');
  let found = 0;
  for (const row of document.querySelectorAll('tr.name')) {
    row.hidden = query !== '' && !row.dataset.find.includes(query);
    found += row.hidden ? 0 : 1;
  }
  // A group's heading shows while any of its names do.
  for (const group of document.querySelectorAll('tr.group')) {
    let shown = false;
    for (let row = group.nextElementSibling; row && !row.classList.contains('group'); row = row.nextElementSibling) {
      shown ||= !row.hidden;
    }
    group.hidden = !shown;
  }
  for (const part of document.querySelectorAll('.named, main section')) {
    part.hidden = query !== '' && !part.querySelector('tr.name:not([hidden])');
  }
  nothing.hidden = query === '' || found > 0;
});
