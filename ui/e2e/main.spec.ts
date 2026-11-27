import { expect, test, type Locator, type Page } from '@playwright/test';

async function runtime(page: Page) {
  const r = await page.request.get('/api/runtime');
  return r.json();
}

/** Adds lights to the scene being edited, from its dialog. */
async function addLights(page: Page, ...boxes: string[]) {
  await page.getByRole('button', { name: 'Add lights…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Add lights' });
  for (const box of boxes) await dialog.getByLabel(`Select ${box}`, { exact: true }).check();
  await dialog.getByRole('button', { name: /^Add \d+ lights?$/ }).click();
  await expect(dialog).toHaveCount(0);
}

/**
 * The lights of the scene (groups and fixtures added on their own), sorted:
 * "Choose lights" lists them, each with a × to remove it.
 */
async function sceneLights(page: Page): Promise<string[]> {
  const names = await page
    .getByRole('group', { name: 'Apply to' })
    .getByRole('button', { name: /^Remove .* from the scene$/ })
    .evaluateAll((bs) => bs.map((b) => (b.getAttribute('aria-label') ?? '').replace(/^Remove (.*) from the scene$/, '$1')));
  return names.sort();
}

/** Opens the scene's ⋯ menu and picks an action. */
async function sceneMenu(page: Page, item: string | RegExp) {
  await page.getByRole('button', { name: 'More actions' }).click();
  await page.getByRole('menuitem', { name: item }).click();
}

/** Puts an intensity in the scene with a quick level of the panel. */
async function quickLevel(page: Page, pct: number) {
  await page.getByRole('group', { name: 'Quick levels' }).getByRole('button', { name: `${pct}%`, exact: true }).click();
}

/** Types a value in a field of the panel and leaves it: it goes into the scene. */
async function enter(page: Page, label: string, value: string) {
  const field = page.getByRole('region', { name: 'Set a value' }).getByLabel(label, { exact: true });
  await field.fill(value);
  await field.press('Tab');
}

/** Shows the scene as every channel on faders. */
async function showChannels(page: Page) {
  await page.getByRole('tab', { name: 'DMX channels' }).click();
}

/** Chooses what the next values of the scene go to: one group or one fixture. */
async function applyTo(page: Page, chip: string) {
  const to = page.getByRole('group', { name: 'Apply to' });
  await to.getByRole('button', { name: 'All lights', exact: true }).click();
  await to.getByRole('button', { name: chip, exact: true }).click();
}

/** A choice among the attributes the scene editor offers. */
const attributeOf = (page: Page, name: string) =>
  page.getByRole('radiogroup', { name: 'Scene attribute' }).getByRole('radio', { name, exact: true });

test('patch, program, blackout and grand master', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();

  // Patch two dimmers from the library, picked by manufacturer then model.
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByLabel('Manufacturer').selectOption('Generic');
  await page.getByLabel('Model').selectOption('Generic/Dimmer');
  const addressField = page.getByLabel('Address');
  await expect(addressField).toHaveValue('1');
  await page.getByLabel('Count').fill('2');
  await page.getByRole('button', { name: 'Patch', exact: true }).click();
  await expect(page.locator('.tree li', { hasText: 'Dimmer 2' })).toBeVisible();
  // The next suggestion is the first free address after the patched dimmers.
  await expect(addressField).toHaveValue('3');

  // Several multi-channel fixtures land one after the other.
  await page.getByLabel('Manufacturer').selectOption('Test');
  await page.getByLabel('Model').selectOption('Test/Spot 540');
  await expect(addressField).toHaveValue('3');
  // The channels of the selected mode are listed.
  const channels = page.getByLabel('Mode channels').locator('tbody tr');
  await expect(channels).toHaveCount(11);
  await expect(channels.nth(1)).toContainText('Pan Fine');
  await page.getByLabel('Count').fill('3');
  await page.getByRole('button', { name: 'Patch', exact: true }).click();
  for (const [n, range] of [[1, '3–13'], [2, '14–24'], [3, '25–35']] as const) {
    await expect(page.locator('.tree li', { hasText: `Spot 540 ${n}` })).toContainText(range);
  }
  await expect(addressField).toHaveValue('36');
  await page.getByLabel('Count').fill('1');

  // QLC+ definitions are in the library, sorted by brand (when downloaded:
  // scripts/update-qlcplus-fixtures.sh; CI always downloads them).
  const library = (await (await page.request.get('/api/library')).json()) as Array<{ manufacturer: string }>;
  if (library.some((e) => e.manufacturer === 'Martin')) {
    await page.getByPlaceholder('Search manufacturer or model…').fill('Martin');
    await expect(page.getByLabel('Manufacturer').locator('option').first()).toContainText('Martin');
    await page.getByPlaceholder('Search manufacturer or model…').fill('');
  } else {
    console.warn('QLC+ library not downloaded: brand search not tested');
  }
  await page.getByLabel('Manufacturer').selectOption('Generic');
  await page.getByLabel('Model').selectOption('Generic/Dimmer');

  // A typed conflicting address is flagged before sending.
  await addressField.fill('1');
  await expect(page.getByText(/Conflicts with/)).toBeVisible();
  await page.getByRole('button', { name: 'First free' }).click();
  await expect(addressField).toHaveValue('36');

  // Simple Desk: select a fixture and set it to full.
  await page.getByRole('button', { name: 'Simple Desk' }).click();
  await page.getByRole('button', { name: 'Dimmer 1', exact: true }).click();
  await page.getByRole('button', { name: 'Full' }).click();
  await expect(page.getByText('Intensity 100%')).toBeVisible();
  await expect.poll(async () => (await runtime(page)).state.programmer.length).toBe(1);

  // Save the programmer as a scene: it appears in the FX screen.
  page.once('dialog', (d) => d.accept('Look 1'));
  await page.getByRole('button', { name: 'Save as scene' }).click();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await expect(page.getByText('Look 1')).toBeVisible();

  // Blackout and grand master are always reachable.
  await page.getByRole('button', { name: 'Blackout' }).click();
  await expect(page.getByRole('button', { name: 'BLACKOUT ON' })).toBeVisible();
  await expect.poll(async () => (await runtime(page)).state.blackout).toBe(true);
  await page.getByRole('button', { name: 'BLACKOUT ON' }).click();
  await expect.poll(async () => (await runtime(page)).state.blackout).toBe(false);
});

test('scene editor, movement preview, knob and DMX channel desk', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();

  // Select heads in the universe tree and create a group.
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByLabel('Select Dimmer 1').check();
  await page.getByLabel('Select Dimmer 2').check();
  page.once('dialog', (d) => d.accept('Front'));
  await page.getByRole('button', { name: 'Create group' }).click();
  // The group is a folder of the universe tree holding its fixtures, and
  // opens its editor.
  const front = page.getByRole('treeitem', { name: 'Group Front' });
  await expect(front).toBeVisible();
  await expect(front).toHaveAttribute('aria-selected', 'true');
  await expect(front).toContainText('2');

  // Reorder the group by dragging heads in the matrix.
  await expect(page.getByLabel('Head Dimmer 1')).toContainText('1');
  await page.getByLabel('Head Dimmer 1').dragTo(page.getByLabel('Head Dimmer 2'));
  await expect(page.getByLabel('Head Dimmer 2')).toContainText('1');
  await expect(page.getByLabel('Head Dimmer 1')).toContainText('2');

  // Settings depend on the fixture type: a dimmer has no orientation.
  await page.locator('.tree li', { hasText: 'Dimmer 1' }).first().locator('button.fx').click();
  await expect(page.getByRole('heading', { name: 'Channels' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Orientation' })).toHaveCount(0);

  // A second group, of moving heads.
  await page.getByLabel('Select Spot 540 1').check();
  await page.getByLabel('Select Spot 540 2').check();
  page.once('dialog', (d) => d.accept('Beams'));
  await page.getByRole('button', { name: 'Create group' }).click();
  await expect(page.getByRole('treeitem', { name: 'Group Beams' })).toBeVisible();

  // Add a group to a scene from the scene editor.
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New scene', exact: true }).click();
  await addLights(page, 'group Front');
  await applyTo(page, 'Group: Front');
  await attributeOf(page, 'Intensity').click();
  await quickLevel(page, 100);
  await expect(page.locator('td', { hasText: 'Intensity 100%' })).toBeVisible();

  // Only what the fixtures can do is offered: dimmers have no zoom, strobe,
  // position or color; moving heads do.
  const attribute = page.getByRole('radiogroup', { name: 'Scene attribute' });
  await expect(attribute.getByRole('radio')).toHaveText(['Intensity']);
  await addLights(page, 'group Beams');
  await applyTo(page, 'Group: Beams');
  await expect(attribute.getByRole('radio')).toHaveText([
    'Intensity', 'Color', 'Position', 'Fan (group order)', 'Gobo', 'Prism', 'Zoom', 'Strobe',
  ]);

  // Regression: quick successive additions must all be kept (each used to
  // be computed on a stale copy of the scene and overwrite the previous one).
  // A control left goes straight into the scene.
  await attributeOf(page, 'Zoom').click();
  await enter(page, 'Zoom degrees', '25');
  await attributeOf(page, 'Strobe').click();
  await enter(page, 'Strobe Hz', '6');
  await attributeOf(page, 'Position').click();
  await enter(page, 'Pan degrees', '10');
  for (const text of ['Intensity 100%', 'Zoom 25°', 'Strobe 6 Hz', 'Pan 10.0° Tilt 0.0°']) {
    await expect(page.locator('td', { hasText: text })).toBeVisible();
  }

  // Choosing them again shows the value they have.
  await attributeOf(page, 'Zoom').click();
  await expect(page.getByRole('region', { name: 'Set a value' }).getByLabel('Zoom degrees')).toHaveValue('25');

  // The other view: one row of faders with every channel of the fixtures it drives.
  await showChannels(page);
  const channels2 = page.getByRole('group', { name: 'Scene channels' });
  await expect(page.getByText('Fixtures in this scene (4)')).toBeVisible();
  const dimmer = channels2.getByLabel('Dimmer 1: Intensity value', { exact: true });
  await expect(dimmer).toHaveValue('255');
  await page.getByRole('treeitem', { name: /Scene 2/ }).getByRole('button', { name: 'Start' }).click();
  await expect(channels2.locator('.strip', { has: page.getByLabel('Dimmer 1: Intensity value', { exact: true }) }).locator('.now')).toHaveText('255');

  // A fader moved by hand is stored in the scene (the summary row comes from
  // the show the engine sends back). Dimmer 2: nothing else holds its output
  // (the programmer still has Dimmer 1 at full).
  const byHandFader = channels2.getByLabel('Dimmer 2: Intensity value', { exact: true });
  const strip = channels2.locator('.strip', { has: page.getByLabel('Dimmer 2: Intensity value', { exact: true }) });
  const byHand = page.getByText('1 channel set by hand');
  // The values view sums the channels set by hand up.
  const byHandShown = async (shown: boolean) => {
    await page.getByRole('tab', { name: 'Values' }).click();
    await expect(byHand).toHaveCount(shown ? 1 : 0);
    await showChannels(page);
  };
  await byHandFader.fill('100');
  await byHandFader.press('Enter');
  await expect(strip).toHaveClass(/\bset\b/);
  await byHandShown(true);
  await expect(strip.locator('.now')).toHaveText('100');
  await channels2.getByRole('slider', { name: 'Dimmer 2: Intensity', exact: true }).fill('90');
  await expect(strip.locator('.now')).toHaveText('90');
  await expect(byHandFader).toHaveValue('90');
  // Releasing gives the channel back to the values above.
  await channels2.getByRole('button', { name: 'Release Dimmer 2: Intensity', exact: true }).click();
  await expect(strip).not.toHaveClass(/\bset\b/);
  await expect(byHandFader).toHaveValue('255');
  await byHandShown(false);
  // Applying a value again releases the channels it writes.
  await byHandFader.fill('100');
  await byHandFader.press('Enter');
  await expect(strip).toHaveClass(/\bset\b/);
  await applyTo(page, 'Group: Front');
  await attributeOf(page, 'Intensity').click();
  await quickLevel(page, 100);
  await expect(strip).not.toHaveClass(/\bset\b/);
  await expect(byHandFader).toHaveValue('255');
  await byHandShown(false);
  await applyTo(page, 'Group: Beams');

  await expect.poll(async () => {
    const show = await (await page.request.get('/api/runtime')).json();
    return show.seq;
  }).toBeGreaterThan(0);

  // A fan on a group is an intention resolved by the core.
  await attributeOf(page, 'Fan (group order)').click();
  await enter(page, 'Pan spread', '50');
  await page.getByRole('tab', { name: 'Values' }).click();
  await expect(page.locator('td', { hasText: /^Fan 50° linear/ })).toBeVisible();

  // Movement effect: the drawing follows the values.
  await page.getByRole('button', { name: 'New movement', exact: true }).click();
  const shape = page.locator('svg[aria-label="Movement preview"] path.shape');
  await expect(shape).toBeVisible();
  const circle = await shape.getAttribute('d');
  // Polygon, sine wave and fan, each with its own setting that redraws the path.
  const shapeMenu = page.getByLabel('Shape', { exact: true });
  await shapeMenu.selectOption('polygon');
  await expect(page.getByLabel('Sides')).toHaveValue('4');
  const square = await shape.getAttribute('d');
  expect(square).not.toBe(circle);
  await page.getByLabel('Sides').fill('6');
  await expect.poll(() => shape.getAttribute('d')).not.toBe(square);
  await shapeMenu.selectOption('sine');
  await expect(page.getByLabel('Sides')).toHaveCount(0);
  await expect(page.getByLabel('Waves')).toHaveValue('2');
  const sine = await shape.getAttribute('d');
  await page.getByLabel('Waves').fill('5');
  await expect.poll(() => shape.getAttribute('d')).not.toBe(sine);
  await shapeMenu.selectOption('fan');
  await expect(page.getByLabel('Hold open and closed')).toHaveValue('10');
  await expect(page.getByLabel('Waves')).toHaveCount(0);
  // A fan is a straight line: drawn, but with no height to be visible.
  await expect(shape).toBeAttached();
  expect(await shape.getAttribute('d')).not.toBe(sine);
  // A fan opens the heads together and is not mirrored.
  await expect(page.getByRole('group', { name: 'Phase presets' }).getByRole('button', { name: 'Together' })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByLabel('Mirrored heads')).toHaveCount(0);
  await expect(page.locator('label.slider', { hasText: 'Opening' })).toHaveCount(1);
  // Lissajous has a slider for each frequency.
  await shapeMenu.selectOption('lissajous');
  await expect(page.getByLabel('Mirrored heads')).toHaveCount(1);
  const lissajous = await shape.getAttribute('d');
  await page.getByLabel('X waves').fill('5');
  await expect.poll(() => shape.getAttribute('d')).not.toBe(lissajous);
  await shapeMenu.selectOption('eight');
  await expect.poll(() => shape.getAttribute('d')).not.toBe(circle);
  const before = await shape.getAttribute('d');
  await page.locator('label.slider', { hasText: 'Width' }).locator('input[type=number]').fill('200');
  await page.locator('label.slider', { hasText: 'Width' }).locator('input[type=number]').press('Enter');
  await expect.poll(() => shape.getAttribute('d')).not.toBe(before);

  // Simple Desk: every channel of the universe can be set on its own.
  await page.getByRole('button', { name: 'Simple Desk' }).click();
  await page.getByRole('button', { name: 'DMX channels' }).click();
  await expect(page.getByLabel('Channel 512', { exact: true })).toBeAttached();
  await page.getByLabel('Channel 300 value').fill('77');
  await page.getByLabel('Channel 300 value').press('Enter');
  await expect
    .poll(async () => (await (await page.request.get('/api/runtime')).json()).state.channels)
    .toContainEqual({ universe: 1, address: 300, value: 77 });
  await page.getByRole('button', { name: 'Release universe' }).click();
  await expect.poll(async () => (await (await page.request.get('/api/runtime')).json()).state.channels).toEqual([]);

  // Virtual console: a knob is a rotary control.
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  await page.getByRole('button', { name: 'Knob' }).click();
  await expect(page.getByRole('slider', { name: 'Knob' })).toBeVisible();

  // Matrix effect preview shows one cell per head of the group.
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New matrix effect', exact: true }).click();
  await expect(page.locator('.matrix .cell')).toHaveCount(2);

  // DMX monitor names fixtures and channels.
  await page.getByRole('button', { name: 'DMX monitor' }).click();
  await expect(page.locator('.monitor .cell').first()).toContainText('Dimmer 1');
  await page.getByRole('button', { name: 'Close' }).click();

  // Deleting a universe deletes its fixtures too.
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByRole('button', { name: '+ Universe' }).click();
  await expect(page.getByText('2: Universe 2')).toBeVisible();
  await page.getByLabel('Manufacturer').selectOption('Generic');
  await page.getByLabel('Model').selectOption('Generic/Dimmer');
  await page.locator('section', { hasText: 'Add fixtures' }).getByLabel('Universe').selectOption('2');
  await page.getByPlaceholder('Name (default: model)').fill('Far dimmer');
  await page.getByRole('button', { name: 'Patch', exact: true }).click();
  await expect(page.locator('.tree li', { hasText: 'Far dimmer' })).toBeVisible();
  page.once('dialog', (d) => d.accept());
  await page.getByLabel('Delete universe Universe 2').click();
  await expect(page.getByText('2: Universe 2')).toHaveCount(0);
  await expect(page.locator('.tree li', { hasText: 'Far dimmer' })).toHaveCount(0);
});

test('functions are listed by type with nestable folders', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  const tree = page.getByRole('tree');
  await expect(tree.getByText('Scenes', { exact: true })).toBeVisible();
  // Types without anything in them are not listed.
  await expect(tree.getByText('Chasers', { exact: true })).toHaveCount(0);
  await expect(tree.getByText('Cue lists', { exact: true })).toHaveCount(0);
  // A folder alone makes its type appear.
  page.once('dialog', (d) => d.accept('Songs'));
  await page.getByLabel('New folder in').selectOption('cue_list');
  await expect(tree.getByText('Cue lists', { exact: true })).toBeVisible();

  // A folder in Scenes, a subfolder inside it, and a new scene in the subfolder.
  page.once('dialog', (d) => d.accept('Act 1'));
  await page.getByRole('button', { name: 'New scene folder' }).click();
  await expect(page.getByLabel('Folder Act 1', { exact: true })).toBeVisible();
  page.once('dialog', (d) => d.accept('Song A'));
  await page.getByRole('button', { name: 'New subfolder in Act 1' }).click();
  await expect(page.getByLabel('Folder Song A', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'New function in Song A' }).click();
  await expect(page.getByLabel('Folder Song A', { exact: true })).toContainText('1');
  // Functions are moved by dragging them in the list only.
  await expect(page.getByRole('button', { name: 'More actions' })).toBeVisible();
  await expect(page.getByLabel('Folder', { exact: true })).toHaveCount(0);

  // Regression: two creations in a row must not reuse the same id (the
  // second used to overwrite the first before the show update arrived).
  await page.getByRole('button', { name: 'New function in Act 1' }).click();
  await expect(page.getByLabel('Folder Act 1', { exact: true })).toContainText('2');
  await expect(page.getByLabel('Folder Song A', { exact: true })).toContainText('1');

  // Drag an existing scene ("Look 1", top level) into "Act 1", back to the top
  // level on its type's header, and in again.
  const look = tree.getByRole('treeitem', { name: /Look 1/ });
  const act = page.getByLabel('Folder Act 1', { exact: true });
  await expect(look).toHaveAttribute('style', /--depth: ?1/);
  await look.dragTo(act);
  await expect(act).toContainText('3');
  await expect(look).toHaveAttribute('style', /--depth: ?2/);
  await look.dragTo(tree.locator('.head', { hasText: 'Scenes' }));
  await expect(act).toContainText('2');
  await expect(look).toHaveAttribute('style', /--depth: ?1/);
  await look.dragTo(act);
  await expect(look).toHaveAttribute('style', /--depth: ?2/);

  // Deleting "Act 1" moves its content up one level.
  page.once('dialog', (d) => d.accept());
  await page.getByRole('button', { name: 'Delete folder Act 1' }).click();
  await expect(page.getByLabel('Folder Act 1', { exact: true })).toHaveCount(0);
  await expect(page.getByLabel('Folder Song A', { exact: true })).toBeVisible();
  await expect(look).toHaveAttribute('style', /--depth: ?1/);
});


// Regressions: a dropped widget flashed back to its start for one round trip
// to the engine, and the clock widget never ticked.
test('virtual console: a dropped widget stays put and the clock ticks', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  const toolbar = page.locator('.toolbar');

  await toolbar.getByRole('button', { name: 'Clock', exact: true }).click();
  const clock = page.locator('.w.clock .big').last();
  const shown = await clock.textContent();
  await expect.poll(() => clock.textContent(), { timeout: 3000 }).not.toBe(shown);

  await toolbar.getByRole('button', { name: 'Label', exact: true }).click();
  const w = page.locator('.w.label').last();
  const styleLeft = () => w.evaluate((el) => parseFloat((el as HTMLElement).style.left));
  const start = await styleLeft();
  const box = (await w.boundingBox())!;
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 85, box.y + 5, { steps: 5 });
  // Record where the widget is drawn on every frame from just before the drop.
  await w.evaluate((el) => {
    const seen: number[] = [];
    (window as unknown as { lefts: number[] }).lefts = seen;
    const frame = () => {
      seen.push(el.getBoundingClientRect().left);
      if (seen.length < 60) requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  });
  await page.mouse.up();
  await page.waitForFunction(() => (window as unknown as { lefts: number[] }).lefts.length >= 60);
  const lefts = await page.evaluate(() => (window as unknown as { lefts: number[] }).lefts);
  expect(Math.min(...lefts)).toBeGreaterThan(box.x + 40);
  await expect.poll(styleLeft).toBe(start + 80);
});

test('a channel with presets opens a list and the fader jumps to the chosen one', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByLabel('Manufacturer').selectOption('Test');
  await page.getByLabel('Model').selectOption('Test/Spot 540');
  await page.getByRole('button', { name: 'Patch', exact: true }).click();

  await page.getByRole('button', { name: 'Simple Desk' }).click();
  await page.getByRole('button', { name: 'DMX channels' }).click();
  await page.getByRole('button', { name: 'Patched', exact: true }).click();

  // The color wheel (5th channel of the mode) offers its slots; Pan does not.
  const wheel = page.getByRole('button', { name: /^Presets of channel/ }).first();
  const address = Number(/\d+/.exec((await wheel.getAttribute('aria-label')) ?? '')![0]);
  await expect(page.getByRole('button', { name: `Presets of channel ${address - 4}`, exact: true })).toHaveCount(0);
  await wheel.click();
  await expect(page.getByRole('option', { name: /Red/ })).toBeVisible();
  await page.getByRole('option', { name: /Red/ }).click();

  // "Red" is 10-19: the channel goes to the middle of it, as scenes do.
  await expect(page.getByLabel(`Channel ${address}`, { exact: true })).toHaveValue('14');
  await expect
    .poll(async () => (await (await page.request.get('/api/runtime')).json()).state.channels)
    .toContainEqual({ universe: 1, address, value: 14 });
  // The strip names the slot now selected, and the list highlights it.
  await expect(page.locator('.strip.set .ch', { hasText: 'Red' }).first()).toBeVisible();
  await wheel.click();
  await expect(page.getByRole('option', { name: /Red/ })).toHaveAttribute('aria-selected', 'true');
  // Regression: focus went to the first preset instead of the current one.
  await expect(page.getByRole('option', { name: /Red/ })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('listbox')).toHaveCount(0);

  // Regression: opened upwards (button low in the window), the list covered
  // the button that opened it.
  await page.setViewportSize({ width: 1300, height: 320 });
  await wheel.scrollIntoViewIfNeeded();
  await wheel.click();
  const button = (await wheel.boundingBox())!;
  const list = (await page.getByRole('listbox').boundingBox())!;
  expect(button.y + button.height).toBeGreaterThan(320 * 0.55);
  expect(list.y + list.height).toBeLessThanOrEqual(button.y + 0.5);
  await page.keyboard.press('Escape');
});

test('a color wheel fixture offers its wheel colors instead of a free picker', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByLabel('Manufacturer').selectOption('Test');
  await page.getByLabel('Model').selectOption('Test/Spot 540');
  await page.getByPlaceholder('Name (default: model)').fill('Wheel spot');
  await page.getByRole('button', { name: 'Patch', exact: true }).click();

  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New scene', exact: true }).click();
  await addLights(page, 'Wheel spot');
  // No palette exists yet, so none is offered (it would be an empty list).
  await expect(page.getByRole('radiogroup', { name: 'Scene attribute' }).getByRole('radio')).toHaveText([
    'Intensity', 'Color', 'Position', 'Gobo', 'Prism', 'Zoom', 'Strobe',
  ]);
  await attributeOf(page, 'Color').click();

  // The slots of the wheel, no free color input.
  const wheel = page.getByRole('radiogroup', { name: 'Color wheel colors' });
  for (const name of ['White', 'Red', 'Green', 'Blue', 'Amber']) {
    await expect(wheel.getByRole('radio', { name, exact: true })).toBeVisible();
  }
  await expect(wheel.getByRole('radio')).toHaveCount(5);
  await expect(page.locator('input[type=color]')).toHaveCount(0);

  await wheel.getByRole('radio', { name: 'Blue', exact: true }).click();
  await expect(wheel.getByRole('radio', { name: 'Blue', exact: true })).toHaveAttribute('aria-checked', 'true');
  // A wheel color picked goes straight into the scene.
  await expect(page.locator('td', { hasText: 'Color #0000ff' })).toBeVisible();

  // The engine puts the wheel on that slot: Blue is 30-39, so 34.
  await showChannels(page);
  const fader = page.getByLabel('Wheel spot: Color Wheel value', { exact: true });
  await expect(fader).toHaveValue('34');

  // The channel still offers its slots, as in the Simple Desk.
  await page.getByRole('button', { name: 'Presets of Wheel spot: Color Wheel', exact: true }).click();
  await page.getByRole('option', { name: /Red/ }).click();
  await expect(fader).toHaveValue('14');
  // One byte of a 16-bit channel set by hand: the scene drives the other too.
  const finePan = page.locator('.strip', { has: page.getByLabel('Wheel spot: Pan Fine value', { exact: true }) });
  await expect(finePan).toHaveClass(/\bunset\b/);
  const pan = page.getByLabel('Wheel spot: Pan value', { exact: true });
  await pan.fill('100');
  await pan.press('Enter');
  await expect(finePan).toHaveClass(/\bpaired\b/);
  await expect(finePan.getByRole('button', { name: /^Release/ })).toBeDisabled();
  // Regression: the dashed border removed the thick one that marks where a
  // fixture starts. Pan is its first channel: paired once only Pan Fine is set.
  const finePanValue = page.getByLabel('Wheel spot: Pan Fine value', { exact: true });
  await finePanValue.fill('7');
  await finePanValue.press('Enter');
  await expect(finePan).toHaveClass(/\bset\b/);
  await page.getByRole('button', { name: 'Release Wheel spot: Pan', exact: true }).click();
  const coarsePan = page.locator('.strip', { has: pan });
  await expect(coarsePan).toHaveClass(/\bpaired\b/);
  await expect(coarsePan).toHaveCSS('border-left-width', '3px');
  await expect(coarsePan).toHaveCSS('border-top-style', 'dashed');
  // One row of faders, scrolling sideways, that does not widen the page.
  const strips = page.getByRole('group', { name: 'Scene channels' });
  await page.setViewportSize({ width: 950, height: 720 });
  expect(await strips.evaluate((e) => e.scrollWidth > e.clientWidth)).toBe(true);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  const tops = await strips.locator('.strip').evaluateAll((all) => new Set(all.map((e) => e.getBoundingClientRect().top)).size);
  expect(tops).toBe(1);
  await page.setViewportSize({ width: 1280, height: 720 });

  // Once a palette is stored (Simple Desk), the editor offers it.
  await page.getByRole('button', { name: 'Simple Desk' }).click();
  await page.getByRole('button', { name: 'Wheel spot', exact: true }).click();
  await page.getByRole('button', { name: 'Full' }).click();
  await page.locator('select').filter({ has: page.locator('option', { hasText: 'Beam' }) }).selectOption('Intensity');
  page.once('dialog', (d) => d.accept('Bright'));
  await page.getByRole('button', { name: 'New palette' }).click();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New scene', exact: true }).click();
  await addLights(page, 'Wheel spot');
  // Regression: with palettes in the show the editor opened on "Palette".
  await expect(attributeOf(page, 'Intensity')).toBeChecked();
  await attributeOf(page, 'Palette').click();
  await expect(page.locator('select option', { hasText: 'Bright (Intensity)' })).toBeAttached();
});

test('chaser steps are picked in a dialog and can be reordered', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  const name = page.locator('.editor input').first();
  for (const scene of ['Step A', 'Step B', 'Step C']) {
    await page.getByRole('button', { name: 'New scene', exact: true }).click();
    await expect(name).toHaveValue(/^Scene \d+$/);
    await name.fill(scene);
    await name.press('Enter');
    await expect(page.getByRole('treeitem', { name: new RegExp(scene) })).toBeVisible();
  }

  // A new chaser opens the dialog; scenes are added in the order of the ticks.
  await page.getByRole('button', { name: 'New chaser', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  // Chasers and cue lists cannot be steps.
  await expect(dialog.getByRole('treeitem', { name: /Chasers|Cue lists/ })).toHaveCount(0);
  await dialog.getByLabel('Select Step B').check();
  await dialog.getByLabel('Select Step A').check();
  // Steps are ordered on the chaser screen only.
  await expect(dialog.getByRole('button', { name: /^Move / })).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Add 2 steps' }).click();
  await expect(dialog).toHaveCount(0);
  const steps = page.getByRole('list', { name: 'Chaser steps' }).locator('li .name');
  await expect(steps).toHaveText(['Step B', 'Step A']);

  // Reopened, it appends: a scene can appear more than once.
  await page.getByRole('button', { name: 'Add steps…' }).click();
  await dialog.getByLabel('Select Step C').check();
  await dialog.getByLabel('Select Step B').check();
  await dialog.getByRole('button', { name: 'Add 2 steps' }).click();
  await expect(steps).toHaveText(['Step B', 'Step A', 'Step C', 'Step B']);

  // Hold and fade are picked from a list of beat lengths (the fade also 0).
  const step = page.getByLabel('Hold', { exact: true });
  const fade = page.getByLabel('Fade', { exact: true });
  const lengths = ['1/32', '1/16', '1/8', '1/4', '1/2'].map((n) => `${n} beat`).concat('1 beat', ['2', '4', '8', '16', '32'].map((n) => `${n} beats`));
  await expect(step.locator('option')).toHaveText(lengths);
  await expect(fade.locator('option')).toHaveText(['0', ...lengths]);
  await expect(step.locator('option:checked')).toHaveText('1 beat');
  await expect(fade.locator('option:checked')).toHaveText('0');
  await fade.selectOption({ label: '1/4 beat' });
  await expect(fade.locator('option:checked')).toHaveText('1/4 beat');
  await fade.selectOption({ label: '0' });

  // Cancel changes nothing.
  await page.getByRole('button', { name: 'Add steps…' }).click();
  await dialog.getByLabel('Select Step A').check();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(steps).toHaveCount(4);

  // Movement and matrix effects can be steps too; their rows name the type.
  await page.getByRole('button', { name: 'Add steps…' }).click();
  await dialog.getByLabel(/^Select Movement \d+$/).first().check();
  await dialog.getByLabel(/^Select Matrix \d+$/).first().check();
  await dialog.getByRole('button', { name: 'Add 2 steps' }).click();
  await expect(steps).toHaveCount(6);
  await expect(steps.nth(4)).toHaveText(/^Movement \d+$/);
  const rows = page.getByRole('list', { name: 'Chaser steps' }).locator('li');
  await expect(rows.nth(4).locator('.type')).toHaveText('Movement');
  await expect(rows.nth(5).locator('.type')).toHaveText('Matrix');
  // Without a color of their own, functions show an icon of their type.
  await expect(rows.nth(0).locator('[data-icon]')).toHaveAttribute('data-icon', 'scene');
  await expect(rows.nth(4).locator('[data-icon]')).toHaveAttribute('data-icon', 'movement_effect');
  await expect(rows.nth(5).locator('[data-icon]')).toHaveAttribute('data-icon', 'matrix_effect');
  await expect(page.getByText('effects start and stop at once')).toBeVisible();
  // The function tree of the screen shows the same icons.
  for (const [name, icon] of [[/Step A/, 'scene'], [/^Chaser \d+/, 'chaser'], [/^Movement \d+/, 'movement_effect']] as const) {
    await expect(page.getByRole('treeitem', { name }).first().locator('[data-icon]')).toHaveAttribute('data-icon', icon);
  }
  await page.getByRole('button', { name: 'Remove step 6' }).click();
  await expect(steps).toHaveCount(5);
  await page.getByRole('button', { name: 'Remove step 5' }).click();
  await expect(steps).toHaveCount(4);

  // Order: the handle with the keyboard, drag and drop, removal.
  const moveStep = async (n: number, key: 'ArrowUp' | 'ArrowDown') => {
    await page.getByRole('button', { name: `Move step ${n}`, exact: true }).focus();
    await page.keyboard.press(key);
  };
  await moveStep(3, 'ArrowUp');
  await expect(steps).toHaveText(['Step B', 'Step C', 'Step A', 'Step B']);
  await page.getByRole('button', { name: 'Remove step 4' }).click();
  await expect(steps).toHaveText(['Step B', 'Step C', 'Step A']);
  await steps.nth(2).dragTo(steps.nth(0));
  await expect(steps).toHaveText(['Step A', 'Step B', 'Step C']);
  await steps.nth(0).dragTo(steps.nth(2));
  await expect(steps).toHaveText(['Step B', 'Step C', 'Step A']);

  // Regression: two clicks before the engine answers carried the same position,
  // and the second one hit whatever had moved there. Both clicks are sent in the
  // same task. The warning shows once the second one has been handled: only then
  // is the list final.
  const warning = page.getByText('The steps changed in the meantime');
  const clickTwice = async (button: string) => {
    await page.getByRole('button', { name: button, exact: true }).evaluate((b: HTMLButtonElement) => {
      b.click();
      b.click();
    });
    await expect(warning).toBeVisible();
  };
  await page.getByRole('button', { name: 'Move step 3', exact: true }).evaluate((b: HTMLButtonElement) => {
    for (let k = 0; k < 2; k++) b.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }));
  });
  await expect(warning).toBeVisible();
  await expect(steps).toHaveText(['Step B', 'Step A', 'Step C']);
  // The next check needs a warning of its own.
  await expect(warning).toHaveCount(0);
  await clickTwice('Remove step 1');
  await expect(steps).toHaveText(['Step A', 'Step C']);
  // Back to the previous list for what follows.
  await page.getByRole('button', { name: 'Add steps…' }).click();
  await dialog.getByLabel('Select Step B').check();
  await dialog.getByRole('button', { name: 'Add 1 step' }).click();
  await expect(steps).toHaveText(['Step A', 'Step C', 'Step B']);
  await moveStep(3, 'ArrowUp');
  await expect(steps).toHaveText(['Step A', 'Step B', 'Step C']);
  await moveStep(1, 'ArrowDown');
  await expect(steps).toHaveText(['Step B', 'Step A', 'Step C']);
  await moveStep(3, 'ArrowUp');
  await expect(steps).toHaveText(['Step B', 'Step C', 'Step A']);

  // The order is the engine's, not just the page's.
  await page.reload();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('treeitem', { name: /^Chaser \d+/ }).click();
  await expect(steps).toHaveText(['Step B', 'Step C', 'Step A']);
  await expect(page.getByRole('dialog')).toHaveCount(0);

  // The virtual console still builds a frame with the same dialog.
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  await page.getByRole('button', { name: 'Frame from functions…' }).click();
  await dialog.getByLabel('Solo frame (one at a time)').check();
  await dialog.getByLabel('Frame name').fill('Steps');
  await dialog.getByLabel('Select Step A').check();
  await expect(dialog.getByRole('button', { name: 'Move Step A up' })).toBeVisible();
  await dialog.getByRole('button', { name: 'Create 1 button' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByText('Steps (solo)', { exact: true })).toBeVisible();

  // A running chaser lights the button of the scene its step plays.
  const stepButton = page.locator('.w.button', { hasText: 'Step A' }).last();
  await expect(stepButton).not.toHaveClass(/\bon\b/);
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('treeitem', { name: /^Chaser \d+/ }).getByRole('button', { name: 'Start' }).click();
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  // It follows the steps: on, off for the other scenes, on again.
  await expect(stepButton).toHaveClass(/\bon\b/);
  await expect(stepButton).not.toHaveClass(/\bon\b/);
  await expect(stepButton).toHaveClass(/\bon\b/);
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('treeitem', { name: /^Chaser \d+/ }).getByRole('button', { name: 'Stop' }).click();
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  await expect(stepButton).not.toHaveClass(/\bon\b/);
});

test('the function of a virtual console button is picked in a dialog', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  await page.locator('.toolbar').getByRole('button', { name: 'Button', exact: true }).click();
  const field = page.getByRole('button', { name: /^Function: / });
  await expect(field).toHaveAccessibleName('Function: — none —');

  await field.click();
  const dialog = page.getByRole('dialog');
  const assign = dialog.getByRole('button', { name: 'Assign' });
  await expect(assign).toBeDisabled();
  // One choice only: no group boxes, and picking another replaces the first.
  await expect(dialog.getByRole('checkbox')).toHaveCount(0);
  const first = dialog.getByRole('radio', { name: 'Select Step A', exact: true });
  const second = dialog.getByRole('radio', { name: 'Select Step B', exact: true });
  await first.check();
  await second.check();
  await expect(first).not.toBeChecked();
  await assign.click();
  await expect(dialog).toHaveCount(0);
  await expect(field).toHaveAccessibleName('Function: Step B');
  // A button with the default label takes the function's name.
  await expect(page.getByLabel('Label', { exact: true })).toHaveValue('Step B');

  // Reopened, the current function is chosen; Cancel changes nothing.
  await field.click();
  await expect(second).toBeChecked();
  await expect(second).toBeFocused();
  await first.check();
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await expect(field).toHaveAccessibleName('Function: Step B');

  await page.getByRole('button', { name: 'Remove function' }).click();
  await expect(field).toHaveAccessibleName('Function: — none —');

  // A widget bound to a chaser or an effect edits its times; a scene has none.
  const choose = async (kind: string) => {
    await field.click();
    await dialog.getByRole('radio', { name: new RegExp(`^Select ${kind}`) }).first().check();
    await assign.click();
    // The panel follows once the engine has the new function.
    await expect(field).toHaveAccessibleName(new RegExp(`^Function: ${kind}`));
  };
  const step = page.getByLabel('Hold', { exact: true });
  const cycle = page.getByLabel('Cycle', { exact: true });
  await choose('Step A');
  await expect(step).toHaveCount(0);
  await expect(cycle).toHaveCount(0);
  await choose('Movement');
  await cycle.selectOption({ label: '16 beats' });
  await expect(cycle.locator('option:checked')).toHaveText('16 beats');
  await choose('Matrix');
  await cycle.selectOption({ label: '1/8 beat' });
  await expect(cycle.locator('option:checked')).toHaveText('1/8 beat');
  await choose('Chaser');
  await expect(cycle).toHaveCount(0);
  await step.selectOption({ label: '8 beats' });
  // Two quick edits are both kept.
  await page.getByLabel('Fade', { exact: true }).selectOption({ label: '2 beats' });
  // It is the function itself: the FX screen shows the same values.
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('treeitem', { name: /^Chaser \d+/ }).click();
  await expect(step.locator('option:checked')).toHaveText('8 beats');
  await expect(page.getByLabel('Fade', { exact: true }).locator('option:checked')).toHaveText('2 beats');
  await page.getByRole('treeitem', { name: /^Movement \d+/ }).click();
  await expect(cycle.locator('option:checked')).toHaveText('16 beats');
  await page.getByRole('treeitem', { name: /^Matrix \d+/ }).click();
  await expect(cycle.locator('option:checked')).toHaveText('1/8 beat');
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  await page.locator('.w.button').last().click();
  await page.getByRole('button', { name: 'Remove function' }).click();

  // Regression: a list longer than the dialog squeezed the groups, cutting off
  // and overlapping their rows, instead of scrolling.
  await page.setViewportSize({ width: 1280, height: 420 });
  await field.click();
  const tree = dialog.getByRole('tree');
  expect(await tree.evaluate((e) => e.scrollHeight > e.clientHeight)).toBe(true);
  const cut = await tree.locator('.category').evaluateAll((all) => all.filter((e) => e.scrollHeight > e.clientHeight).length);
  expect(cut).toBe(0);
  // The last row can be reached and chosen.
  const last = dialog.getByRole('radio').last();
  await last.check();
  await expect(last).toBeInViewport({ ratio: 1 });
});

test('a collection plays functions together and can be a chaser step', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  const dialog = page.getByRole('dialog');
  const name = page.locator('.editor input').first();

  // A new collection asks what it plays: scenes and effects, nothing that
  // contains other functions.
  await page.getByRole('button', { name: 'New collection', exact: true }).click();
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('treeitem', { name: /Movements/ })).toBeVisible();
  await expect(dialog.getByRole('treeitem', { name: /Chasers|Cue lists|Collections/ })).toHaveCount(0);
  await dialog.getByLabel('Select Step A', { exact: true }).check();
  await dialog.getByLabel('Select Step B', { exact: true }).check();
  await dialog.getByRole('button', { name: 'Add 2 functions' }).click();
  await expect(dialog).toHaveCount(0);
  const members = page.getByRole('list', { name: 'Collection functions' }).locator('li');
  await expect(members.locator('.name')).toHaveText(['Step A', 'Step B']);
  await name.fill('Both looks');
  await name.press('Enter');
  await expect(page.getByRole('treeitem', { name: /Both looks/ }).locator('[data-icon]')).toHaveAttribute('data-icon', 'collection');

  // Members are offered once only.
  await page.getByRole('button', { name: 'Add functions…' }).click();
  await expect(dialog.getByLabel('Select Step C', { exact: true })).toBeVisible();
  await expect(dialog.getByLabel('Select Step A', { exact: true })).toHaveCount(0);
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Remove Step B' }).click();
  await expect(members.locator('.name')).toHaveText(['Step A']);

  // A chaser can step through it.
  await page.getByRole('button', { name: 'New chaser', exact: true }).click();
  await dialog.getByLabel('Select Both looks', { exact: true }).check();
  await dialog.getByRole('button', { name: 'Add 1 step' }).click();
  await expect(dialog).toHaveCount(0);
  const steps = page.getByRole('list', { name: 'Chaser steps' }).locator('li');
  await expect(steps.locator('.type')).toHaveText('Collection');
  await name.fill('Through collections');
  await name.press('Enter');
  await page.getByRole('treeitem', { name: /Both looks/ }).click();

  // Running, it lights the buttons of what it plays.
  await page.getByRole('treeitem', { name: /Both looks/ }).getByRole('button', { name: 'Start' }).click();
  await expect(members.first()).toHaveClass(/current/);
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  const stepButton = page.locator('.w.button', { hasText: 'Step A' }).last();
  await expect(stepButton).toHaveClass(/\bon\b/);
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('treeitem', { name: /Both looks/ }).getByRole('button', { name: 'Stop' }).click();
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  await expect(stepButton).not.toHaveClass(/\bon\b/);
});

test('virtual console: widgets are resized by their bottom right corner', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  const toolbar = page.locator('.toolbar');
  const style = (w: Locator, key: 'left' | 'top' | 'width' | 'height') =>
    w.evaluate((el, k) => parseFloat((el as HTMLElement).style[k]), key);
  const geometry = async (w: Locator) => [await style(w, 'left'), await style(w, 'top'), await style(w, 'width'), await style(w, 'height')];
  /** Drags the corner of a widget by (dx, dy). */
  const drag = async (w: Locator, dx: number, dy: number) => {
    const handle = w.locator(':scope > .resize');
    await handle.scrollIntoViewIfNeeded();
    const box = (await handle.boundingBox())!;
    const [x, y] = [box.x + box.width - 5, box.y + box.height - 5];
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x + dx, y + dy, { steps: 5 });
    await page.mouse.up();
  };

  /** Adds a widget and returns it, once it is on the screen: earlier tests left
   * widgets of the same kinds. */
  const add = async (name: string, selector: string) => {
    const all = page.locator(selector);
    const n = await all.count();
    await toolbar.getByRole('button', { name, exact: true }).click();
    await expect(all).toHaveCount(n + 1);
    return all.nth(n);
  };

  // Steps of the grid, the position unchanged.
  const knob = await add('Knob', '.w.knob');
  await expect.poll(() => geometry(knob).then((g) => g.slice(2))).toEqual([100, 130]);
  const [left, top] = await geometry(knob);
  const dial = knob.getByRole('slider');
  const before = (await dial.boundingBox())!.width;
  await drag(knob, 63, 48);
  await expect.poll(() => geometry(knob)).toEqual([left, top, 160, 180]);
  // The knob grows with its widget.
  expect((await dial.boundingBox())!.width).toBeGreaterThan(before + 30);
  // Never below the minimum, and the content still fits.
  await drag(knob, -400, -400);
  await expect.poll(() => geometry(knob)).toEqual([left, top, 40, 30]);
  await drag(knob, 80, 120);
  await expect.poll(() => geometry(knob)).toEqual([left, top, 120, 150]);

  // A button inside a frame: neither the frame moves nor changes.
  const frame = await add('Frame', '.w.frame');
  await expect.poll(() => geometry(frame).then((g) => g.slice(2))).toEqual([360, 240]);
  const framed = await geometry(frame);
  await toolbar.getByRole('button', { name: 'Button', exact: true }).click();
  const button = frame.locator('.w.button');
  await expect(button).toHaveCount(1);
  await drag(button, 50, 20);
  await expect.poll(() => geometry(button)).toEqual([10, 24, 150, 90]);
  expect(await geometry(frame)).toEqual(framed);
  // The frame is resized by its own corner, its content staying where it is.
  await drag(frame, -40, 30);
  await expect.poll(() => geometry(frame)).toEqual([framed[0], framed[1], 320, 270]);
  expect(await geometry(button)).toEqual([10, 24, 150, 90]);

  // The settings panel and the engine have the new size.
  await expect(page.getByLabel('W', { exact: true })).toHaveValue('320');
  await page.reload();
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  await expect.poll(() => geometry(page.locator('.w.frame').last())).toEqual([framed[0], framed[1], 320, 270]);

  // Regression: a change made before the engine had confirmed the previous one
  // started from the old size, and saved over it. All the events are sent in
  // the same task, before any confirmation can arrive.
  const last = page.locator('.w.frame').last();
  await last.evaluate((frame) => {
    const drag = (w: Element, dx: number, dy: number) => {
      const handle = w.querySelector(':scope > .resize')!;
      const send = (type: string, x: number, y: number) =>
        handle.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerId: 1, button: 0, clientX: x, clientY: y }));
      send('pointerdown', 0, 0);
      send('pointermove', dx, dy);
      send('pointerup', dx, dy);
    };
    const inner = frame.querySelector('.w.button')!;
    drag(inner, 30, 0);
    drag(inner, 0, 20);
    drag(frame, 20, 0);
  });
  // A second page shows what the engine has, with no preview of its own.
  const other = await page.context().newPage();
  await other.goto('/');
  await other.getByRole('button', { name: 'Virtual Console' }).click();
  const there = other.locator('.w.frame').last();
  await expect.poll(() => geometry(there)).toEqual([framed[0], framed[1], 340, 270]);
  await expect.poll(() => geometry(there.locator('.w.button'))).toEqual([10, 24, 180, 110]);
  await other.close();

  // Live, in Show mode, there is nothing to drag.
  await expect(page.locator('.w .resize').first()).toBeVisible();
  await page.getByRole('button', { name: 'Design mode' }).click();
  await expect(page.locator('.w .resize')).toHaveCount(0);
  await page.getByRole('button', { name: 'SHOW mode' }).click();
  await expect(page.locator('.w .resize').first()).toBeVisible();
});

test('the master bar stays still when the mode or the blackout changes', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  const bar = page.getByLabel('Master controls');
  const button = (name: string) => bar.getByRole('button', { name, exact: true });
  /** Where every control of the bar is, and how tall. */
  const layout = () =>
    bar.locator('button, .gm').evaluateAll((all) => all.map((e) => [e.getBoundingClientRect().top, e.getBoundingClientRect().height]));
  await expect(button('Design mode')).toBeVisible();
  const before = await layout();
  // This show has no PIN: nothing is ever asked.
  const dialogs: string[] = [];
  page.on('dialog', (d) => {
    dialogs.push(d.message());
    void d.accept('');
  });

  await button('Blackout').click();
  await expect(button('BLACKOUT ON')).toBeVisible();
  expect(await layout()).toEqual(before);
  await button('Design mode').click();
  await expect(button('SHOW mode')).toBeVisible();
  expect(await layout()).toEqual(before);

  await button('BLACKOUT ON').click();
  await expect(button('Blackout')).toBeVisible();
  await button('SHOW mode').click();
  await expect(button('Design mode')).toBeVisible();
  expect(dialogs).toEqual([]);
});

test('a scene has its lights, and values for all of them or for some', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New scene', exact: true }).click();
  const name = page.locator('.editor input').first();
  await expect(name).toHaveValue(/^Scene \d+$/);
  const channels = page.getByRole('group', { name: 'Scene channels' });
  const fader = (label: string) => channels.getByLabel(`${label} value`, { exact: true });
  const strip = (label: string) => channels.locator('.strip', { has: page.getByLabel(`${label} value`, { exact: true }) });
  // Without lights there is nothing to give values to: only the lights are
  // asked for.
  await expect(page.getByText('No lights yet')).toBeVisible();
  await expect(page.getByRole('region', { name: 'Set a value' })).toHaveCount(0);
  await expect(page.getByRole('tab', { name: 'Values' })).toHaveCount(0);
  await expect(page.getByRole('heading', { name: /^Fixtures in this scene/ })).toHaveCount(0);
  const showValues = () => page.getByRole('tab', { name: 'Values' }).click();

  // The dialog lists groups with their fixtures: a ticked group takes them.
  await page.getByRole('button', { name: 'Add lights…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Add lights' });
  await dialog.getByLabel('Select group Front', { exact: true }).check();
  await expect(dialog.getByLabel('Select Dimmer 1', { exact: true })).toBeChecked();
  await expect(dialog.getByLabel('Select Dimmer 1', { exact: true })).toBeDisabled();
  await dialog.getByLabel('Select Spot 540 1', { exact: true }).check();
  await dialog.getByRole('button', { name: 'Add 2 lights' }).click();
  await expect.poll(() => sceneLights(page)).toEqual(['Front', 'Spot 540 1'].sort());
  // One list: a fixture there through a group has no × of its own.
  await expect(page.getByRole('button', { name: 'Remove Dimmer 1 from the scene' })).toHaveCount(0);
  await expect(page.getByRole('group', { name: 'Apply to' }).getByRole('button', { name: 'Dimmer 1', exact: true })).toBeVisible();

  // A value goes to all lights unless some are chosen.
  await quickLevel(page, 100);
  await expect(page.locator('tr', { hasText: 'All lights' })).toContainText('Intensity 100%');
  await showChannels(page);
  await expect(fader('Dimmer 1: Intensity')).toHaveValue('255');
  await expect(fader('Spot 540 1: Dimmer')).toHaveValue('255');

  // What is in the scene cannot be added again; a new light takes the value.
  await page.getByRole('button', { name: 'Add lights…' }).click();
  await expect(dialog.getByLabel('Select group Front', { exact: true })).toBeDisabled();
  await expect(dialog.getByLabel('Select Spot 540 1', { exact: true })).toBeDisabled();
  await page.keyboard.press('Escape');
  await addLights(page, 'Spot 540 2');
  await expect(fader('Spot 540 2: Dimmer')).toHaveValue('255');

  // Unless the box says otherwise: the value stays with the lights it had.
  await page.getByLabel('Lights added later take the values of all lights').uncheck();
  await addLights(page, 'Wheel spot');
  await expect.poll(() => sceneLights(page)).toEqual(['Front', 'Spot 540 1', 'Spot 540 2', 'Wheel spot'].sort());
  await expect(strip('Wheel spot: Dimmer')).toHaveClass(/\bunset\b/);
  await expect(fader('Spot 540 2: Dimmer')).toHaveValue('255');
  await showValues();
  await expect(page.locator('tr', { hasText: 'All lights' })).toHaveCount(0);

  // Several fixtures at once.
  const to = page.getByRole('group', { name: 'Apply to' });
  await to.getByRole('button', { name: 'Spot 540 1', exact: true }).click();
  await to.getByRole('button', { name: 'Wheel spot', exact: true }).click();
  await expect(to.getByRole('button', { name: 'All lights' })).toHaveAttribute('aria-pressed', 'false');
  await attributeOf(page, 'Zoom').click();
  await enter(page, 'Zoom degrees', '25');
  const zoom = page.locator('tr', { hasText: 'Zoom 25°' });
  await expect(zoom).toHaveText([/Spot 540 1/, /Wheel spot/]);
  // Regression: every edit went back to all lights.
  await expect(to.getByRole('button', { name: 'Wheel spot', exact: true })).toHaveAttribute('aria-pressed', 'true');

  // Fixtures are in a section per model, which can be chosen whole; the
  // search finds a light by name.
  await to.getByRole('button', { name: 'All lights', exact: true }).click();
  const model = to.getByRole('button', { name: 'All Spot 540', exact: true });
  await model.click();
  for (const spot of ['Spot 540 1', 'Spot 540 2', 'Wheel spot']) {
    await expect(to.getByRole('button', { name: spot, exact: true })).toHaveAttribute('aria-pressed', 'true');
  }
  await expect(to).toContainText('3 fixtures chosen');
  await expect(model).toHaveAttribute('aria-pressed', 'true');
  await to.getByRole('button', { name: 'Wheel spot', exact: true }).click();
  await expect(model).toHaveAttribute('aria-pressed', 'mixed');
  await expect(model).toContainText('2 of 3');
  await model.click();
  await expect(model).toHaveAttribute('aria-pressed', 'true');
  await model.click();
  await expect(to.getByRole('button', { name: 'All lights' })).toHaveAttribute('aria-pressed', 'true');
  await to.getByLabel('Find a light').fill('wheel');
  await expect(to.getByRole('button', { name: 'Wheel spot', exact: true })).toBeVisible();
  await expect(to.getByRole('button', { name: 'Spot 540 1', exact: true })).toHaveCount(0);
  await expect(to.getByRole('button', { name: 'Group: Front' })).toHaveCount(0);
  await to.getByLabel('Find a light').fill('');
  await expect(to.getByRole('button', { name: 'Group: Front' })).toBeVisible();

  // A removed light takes its values with it.
  await page.getByRole('button', { name: 'Remove Spot 540 1 from the scene' }).click();
  await expect.poll(() => sceneLights(page)).toEqual(['Front', 'Spot 540 2', 'Wheel spot'].sort());
  await expect(zoom).toHaveText([/Wheel spot/]);
  await expect(page.locator('tr', { hasText: 'Spot 540 1' })).toHaveCount(0);
  await showChannels(page);
  await expect(fader('Dimmer 1: Intensity')).toBeVisible();
  await expect(fader('Spot 540 1: Dimmer')).toHaveCount(0);
  await expect(to.getByRole('button', { name: 'Spot 540 1', exact: true })).toHaveCount(0);

  // All of it is in the show.
  await page.reload();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('treeitem', { name: /Scene \d+/ }).last().click();
  await expect.poll(() => sceneLights(page)).toEqual(['Front', 'Spot 540 2', 'Wheel spot'].sort());
  await expect(page.getByLabel('Lights added later take the values of all lights')).not.toBeChecked();
});

test('lights without degrees or frequencies take plain DMX values', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByLabel('Manufacturer').selectOption('Test');
  await page.getByLabel('Model').selectOption('Test/Basic Head');
  await page.getByLabel('Address').fill('200');
  await page.getByRole('button', { name: 'Patch', exact: true }).click();
  await expect(page.locator('.tree li', { hasText: 'Basic Head' })).toBeVisible();

  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New scene', exact: true }).click();
  await addLights(page, 'Basic Head');
  const channels = page.getByRole('group', { name: 'Scene channels' });
  const fader = (channel: string) => channels.getByLabel(`Basic Head: ${channel} value`, { exact: true });

  await attributeOf(page, 'Position').click();
  await expect(page.getByRole('radiogroup', { name: 'Unit' })).toHaveCount(0);
  await enter(page, 'Pan DMX value', '200');
  await enter(page, 'Tilt DMX value', '50');
  await expect(page.locator('td', { hasText: 'Pan DMX 200' })).toBeVisible();
  await expect(page.locator('td', { hasText: 'Tilt DMX 50' })).toBeVisible();
  await showChannels(page);
  await expect(fader('Pan')).toHaveValue('200');
  await expect(fader('Tilt')).toHaveValue('50');

  await attributeOf(page, 'Strobe').click();
  await enter(page, 'Strobe DMX value', '100');
  await expect(fader('Strobe')).toHaveValue('100');
  await page.getByRole('tab', { name: 'Values' }).click();
  await expect(page.locator('td', { hasText: 'Strobe DMX 100' })).toBeVisible();

  // Edit brings back both values of a position.
  await page.getByLabel('Strobe DMX value').fill('0');
  await page.locator('tr', { hasText: 'Tilt DMX 50' }).getByRole('button', { name: 'Edit' }).click();
  await expect(page.getByLabel('Pan DMX value')).toHaveValue('200');
  await expect(page.getByLabel('Tilt DMX value')).toHaveValue('50');

  // With a light that has degrees too, either can be chosen.
  await addLights(page, 'Wheel spot');
  const unit = page.getByRole('radiogroup', { name: 'Unit' });
  await expect(unit.getByRole('radio')).toHaveText(['Degrees', 'DMX 0–255']);
  await unit.getByRole('radio', { name: 'Degrees' }).click();
  await expect(page.getByLabel('Pan DMX value')).toHaveCount(0);
  await expect(page.getByRole('spinbutton', { name: /^Pan/ })).toHaveValue('0');
  // With that light only, degrees and nothing to choose.
  await applyTo(page, 'Wheel spot');
  await expect(unit).toHaveCount(0);
  await expect(page.getByLabel('Pan DMX value')).toHaveCount(0);
});

test('a scene can be duplicated, and the copy changes on its own', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New scene', exact: true }).click();
  const name = page.locator('.editor input').first();
  await expect(name).toHaveValue(/^Scene \d+$/);
  await name.fill('Original');
  await name.press('Enter');
  await expect(page.getByRole('treeitem', { name: /Original/ })).toBeVisible();
  await addLights(page, 'Wheel spot');
  await quickLevel(page, 100);
  const values = page.locator('tr', { hasText: 'Intensity 100%' });
  await expect(values).toHaveCount(1);

  await sceneMenu(page, 'Duplicate');
  await expect(name).toHaveValue('Original copy');
  await expect.poll(() => sceneLights(page)).toEqual(['Wheel spot'].sort());
  await expect(values).toHaveCount(1);

  // Changing the copy leaves the original as it was.
  await values.getByRole('button', { name: '×' }).click();
  await expect(values).toHaveCount(0);
  await page.getByRole('treeitem', { name: /Original(?! copy)/ }).click();
  await expect(name).toHaveValue('Original');
  await expect(values).toHaveCount(1);
  await sceneMenu(page, 'Duplicate');
  await expect(name).toHaveValue('Original copy 2');

  // Every editor offers it in its menu, but collections.
  await page.getByRole('treeitem', { name: /^Chaser \d+/ }).first().click();
  await sceneMenu(page, 'Duplicate');
  await expect(page.locator('.editor input').first()).toHaveValue(/^Chaser \d+ copy$/);
  await page.getByRole('treeitem', { name: /^Movement \d+/ }).first().click();
  await sceneMenu(page, 'Duplicate');
  await expect(page.locator('.editor input').first()).toHaveValue(/^Movement \d+ copy$/);
  await page.getByRole('treeitem', { name: /^Matrix \d+/ }).first().click();
  await sceneMenu(page, 'Duplicate');
  await expect(page.locator('.editor input').first()).toHaveValue(/^Matrix \d+ copy$/);
  // Collections keep the plain header.
  await page.getByRole('treeitem', { name: /Both looks/ }).click();
  await expect(page.getByRole('button', { name: 'Delete', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'More actions' })).toHaveCount(0);
});

test('a scene is started, previewed on the lights and undone from its editor', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  // Nothing left on the lights by the tests before.
  await page.getByRole('button', { name: 'Stop all' }).click();
  await page.getByRole('button', { name: 'Simple Desk' }).click();
  await page.getByRole('button', { name: 'Selection & intentions' }).click();
  await page.getByRole('button', { name: 'Clear', exact: true }).click();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New scene', exact: true }).click();
  const name = page.locator('.editor input').first();
  await expect(name).toHaveValue(/^Scene \d+$/);
  await name.fill('Tryout');
  await name.press('Enter');
  await expect(page.getByRole('treeitem', { name: /Tryout/ })).toBeVisible();
  await addLights(page, 'Wheel spot');
  await showChannels(page);
  const channels = page.getByRole('group', { name: 'Scene channels' });
  const strip = channels.locator('.strip', { has: page.getByLabel('Wheel spot: Dimmer value', { exact: true }) });
  const output = strip.locator('.now');
  const level = page.getByRole('slider', { name: 'Intensity', exact: true });
  await expect(output).toHaveText('0');
  /** The scene holds that value (the values view says so). */
  const stored = async (text: string) => {
    await page.getByRole('tab', { name: 'Values' }).click();
    await expect(page.locator('tr', { hasText: text })).toHaveCount(1);
    await showChannels(page);
  };

  // A control let go goes into the scene, but the lights only show it once
  // asked for (or with the scene started).
  const live = page.getByLabel('Live preview on the lights');
  await expect(live).not.toBeChecked();
  await level.fill('60');
  await stored('Intensity 60%');
  await expect(output).toHaveText('0');
  await live.check();
  await expect(output).toHaveText('153');
  // Choosing what to set changes nothing; moving the control does.
  await attributeOf(page, 'Color').click();
  await attributeOf(page, 'Intensity').click();
  await expect(level).toHaveValue('60');
  await expect(output).toHaveText('153');
  await level.fill('40');
  await expect(output).toHaveText('102');
  // It stays for as long as the editor does (the engine drops it unrenewed).
  await page.waitForTimeout(3500);
  await expect(output).toHaveText('102');
  await stored('Intensity 40%');
  await live.uncheck();
  await expect(output).toHaveText('0');

  // Leaving the screen gives the lights back at once.
  await live.check();
  await level.fill('100');
  await expect(output).toHaveText('255');
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByRole('button', { name: 'DMX monitor' }).click();
  const cell = page.locator('.monitor .cell[title*="Wheel spot — Dimmer"]');
  await expect(cell.locator('.v')).toHaveText('0', { timeout: 2000 });
  await page.getByRole('button', { name: 'Close' }).click();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('treeitem', { name: /Tryout/ }).click();
  await live.uncheck();

  // Started and stopped from the editor.
  await stored('Intensity 100%');
  const header = page.locator('.editor');
  await header.getByRole('button', { name: 'Start', exact: true }).click();
  await expect(output).toHaveText('255');
  await header.getByRole('button', { name: 'Stop', exact: true }).click();
  await expect(output).toHaveText('0');

  // A light removed by mistake comes back.
  await page.getByRole('button', { name: 'Remove Wheel spot from the scene' }).click();
  await expect.poll(() => sceneLights(page)).toEqual([]);
  await page.getByRole('button', { name: 'Undo' }).click();
  await expect.poll(() => sceneLights(page)).toEqual(['Wheel spot'].sort());
  await stored('Intensity 100%');
  await page.getByRole('button', { name: 'Redo' }).click();
  await expect.poll(() => sceneLights(page)).toEqual([]);
  await page.getByRole('button', { name: 'Undo' }).click();
  await expect.poll(() => sceneLights(page)).toEqual(['Wheel spot'].sort());

  // Live, there is no editing to undo.
  await page.getByRole('button', { name: 'Design mode' }).click();
  await expect(page.getByRole('button', { name: 'Undo' })).toBeDisabled();
  await expect(live).toBeDisabled();
  await page.getByRole('button', { name: 'SHOW mode' }).click();
  await expect(page.getByRole('button', { name: 'Undo' })).toBeEnabled();
});

test('scene values with no effect, highlight, times per attribute and tests with fades', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Stop all' }).click();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New scene', exact: true }).click();
  const name = page.locator('.editor input').first();
  await name.fill('Overrides');
  await name.press('Enter');
  await addLights(page, 'Wheel spot');
  await showChannels(page);
  const channels = page.getByRole('group', { name: 'Scene channels' });
  const strip = channels.locator('.strip', { has: page.getByLabel('Wheel spot: Dimmer value', { exact: true }) });
  const output = strip.locator('.now');
  await expect(output).toHaveText('0');
  const to = page.getByRole('group', { name: 'Apply to' });
  const showValues = () => page.getByRole('tab', { name: 'Values' }).click();

  // Highlight: a light at full until it is turned off; nothing stored.
  const bulb = to.getByRole('button', { name: 'Highlight Wheel spot' });
  await bulb.click();
  await expect(bulb).toHaveAttribute('aria-pressed', 'true');
  await expect(output).toHaveText('255');
  await bulb.click();
  await expect(output).toHaveText('0');
  await showValues();
  await expect(page.locator('tr', { hasText: 'Intensity' })).toHaveCount(0);

  // A value to all lights, then one to the fixture: the last one wins, the
  // first has no effect and can be removed without changing the lights.
  // Each light shows the intensity it gets.
  const chip = to.getByRole('button', { name: 'Wheel spot', exact: true });
  await expect(chip).toContainText('–');
  const level = page.getByRole('slider', { name: 'Intensity', exact: true });
  await level.fill('40');
  await expect(chip).toContainText('40%');
  await chip.click();
  await level.fill('100');
  await expect(chip).toContainText('100%');
  const first = page.locator('tr', { hasText: 'Intensity 40%' });
  await expect(first).toHaveClass(/\bcovered\b/);
  await expect(first).toContainText('no effect');
  await expect(page.locator('tr', { hasText: 'Intensity 100%' })).not.toHaveClass(/\bcovered\b/);
  await sceneMenu(page, 'Remove 1 value with no effect');
  await expect(first).toHaveCount(0);
  await expect(page.locator('tr', { hasText: 'Intensity 100%' })).toHaveCount(1);

  // The times are folded into a line at the bottom: own times for the
  // intensity, and the delay spread over the lights.
  await expect(page.getByLabel('Delay over the lights')).toBeHidden();
  await page.locator('summary', { hasText: 'Timing' }).click();
  await page.getByText('Times per attribute').click();
  await page.getByLabel('Intensity: own times').check();
  await expect(page.locator('.kind', { hasText: 'Intensity' }).getByRole('button', { name: /^Fade in/ })).toBeVisible();
  await page.getByLabel('Delay over the lights').selectOption('from_center');
  await expect(page.getByRole('button', { name: 'Step: 1/4 beat' })).toBeVisible();
  await expect(page.locator('summary', { hasText: 'Timing' })).toContainText('own times: intensity');
  await showChannels(page);

  // Starting the scene pauses the live preview, so its own fades show.
  const live = page.getByLabel('Live preview on the lights');
  await live.check();
  const header = page.locator('.editor');
  await header.getByRole('button', { name: 'Start', exact: true }).click();
  await expect(page.getByText('Paused while the scene was started')).toBeVisible();
  await expect(output).toHaveText('255');
  await header.getByRole('button', { name: 'Stop', exact: true }).click();
  await expect(output).toHaveText('0');
  await live.uncheck();
});

test('a chaser shows its timeline, takes times per step and jumps to a step', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Stop all' }).click();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  const name = page.locator('.editor input').first();
  for (const scene of ['Go 1', 'Go 2', 'Go 3']) {
    await page.getByRole('button', { name: 'New scene', exact: true }).click();
    await expect(name).toHaveValue(/^Scene \d+$/);
    await name.fill(scene);
    await name.press('Enter');
    await expect(page.getByRole('treeitem', { name: new RegExp(scene) })).toBeVisible();
    // The first one lights the wheel spot, to be seen in the preview.
    if (scene === 'Go 1') {
      await addLights(page, 'Wheel spot');
      // Regression: renaming right after a change dropped it (the rename saved
      // a copy taken before the change came back). Both in the same task.
      await expect(page.getByRole('group', { name: 'Quick levels' })).toBeVisible();
      await page.evaluate(() => {
        const full = [...document.querySelectorAll<HTMLButtonElement>('[aria-label="Quick levels"] button')].find((b) => b.textContent?.trim() === '100%');
        full?.click();
        const input = document.querySelector<HTMLInputElement>('.editor input');
        if (input) {
          input.value = 'Go 1 lit';
          input.dispatchEvent(new Event('change', { bubbles: true }));
        }
      });
      await expect(page.getByRole('treeitem', { name: /Go 1 lit/ })).toBeVisible();
      await expect(page.locator('tr', { hasText: 'Intensity 100%' })).toHaveCount(1);
      await name.fill('Go 1');
      await name.press('Enter');
    }
  }
  await page.getByRole('button', { name: 'New chaser', exact: true }).click();
  const dialog = page.getByRole('dialog');
  for (const scene of ['Go 1', 'Go 2', 'Go 3']) await dialog.getByLabel(`Select ${scene}`, { exact: true }).check();
  // Regression: renaming right after adding steps dropped them. Same task.
  await expect(dialog.getByRole('button', { name: 'Add 3 steps' })).toBeEnabled();
  await page.evaluate(() => {
    const add = [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find((b) => b.textContent?.trim() === 'Add 3 steps');
    add?.click();
    const input = document.querySelector<HTMLInputElement>('.editor input');
    if (input) {
      input.value = 'Tester';
      input.dispatchEvent(new Event('change', { bubbles: true }));
    }
  });
  await expect(page.getByRole('treeitem', { name: /Tester/ })).toBeVisible();
  const steps = page.getByRole('list', { name: 'Chaser steps' }).locator('li');
  await expect(steps.locator('.name')).toHaveText(['Go 1', 'Go 2', 'Go 3']);
  await page.waitForTimeout(300);
  await expect(steps.locator('.name')).toHaveText(['Go 1', 'Go 2', 'Go 3']);

  // One block per step on the timeline, as long as it lasts.
  const blocks = page.getByRole('region', { name: 'Timeline' }).getByRole('button');
  await expect(blocks).toHaveCount(3);
  await expect(blocks.nth(1)).toContainText('1 beat');

  // A step can have its own hold, and give it back to the chaser.
  const hold2 = page.getByLabel('Hold of step 2', { exact: true });
  await expect(hold2.locator('option:checked')).toHaveText('= 1 beat');
  await hold2.selectOption({ label: '2 beats' });
  await expect(hold2).toHaveClass(/\bown\b/);
  await expect(blocks.nth(1)).toContainText('2 beats');
  await hold2.selectOption({ label: '= 1 beat' });
  await expect(hold2).not.toHaveClass(/\bown\b/);
  await expect(blocks.nth(1)).toContainText('1 beat');
  // Regression: two quick time edits on different steps are both kept (the
  // second was dropped as if the steps had moved).
  await page.evaluate(() => {
    const pick = (label: string, text: string) => {
      const select = document.querySelector<HTMLSelectElement>(`select[aria-label="${label}"]`);
      const option = [...(select?.options ?? [])].find((o) => o.text === text);
      if (select && option) {
        select.value = option.value;
        select.dispatchEvent(new Event('change', { bubbles: true }));
      }
    };
    pick('Hold of step 1', '2 beats');
    pick('Fade of step 3', '1/2 beat');
  });
  await expect(page.getByLabel('Hold of step 1', { exact: true })).toHaveClass(/\bown\b/);
  await expect(page.getByLabel('Fade of step 3', { exact: true })).toHaveClass(/\bown\b/);
  await expect(page.getByText('The steps changed in the meantime')).toHaveCount(0);
  await page.getByLabel('Hold of step 1', { exact: true }).selectOption({ label: '= 1 beat' });
  await page.getByLabel('Fade of step 3', { exact: true }).selectOption({ label: '= 0' });

  // A step can be duplicated next to itself.
  await page.getByRole('button', { name: 'Duplicate step 1', exact: true }).click();
  await expect(steps.locator('.name')).toHaveText(['Go 1', 'Go 1', 'Go 2', 'Go 3']);
  await page.getByRole('button', { name: 'Remove step 2', exact: true }).click();
  await expect(steps.locator('.name')).toHaveText(['Go 1', 'Go 2', 'Go 3']);

  // Running: the current step is marked; a click, or the step buttons, jump.
  // Long steps, so that it does not move on by itself meanwhile.
  await page.getByLabel('Hold', { exact: true }).selectOption({ label: '32 beats' });
  const header = page.locator('.editor .head');
  await header.getByRole('button', { name: 'Start', exact: true }).click();
  await expect(steps.nth(0)).toHaveClass(/\bcurrent\b/);
  await expect(blocks.nth(0)).toHaveAttribute('aria-current', 'step');
  await steps.nth(2).locator('button.pick').click();
  await expect(steps.nth(2)).toHaveClass(/\bcurrent\b/);
  await expect(steps.nth(0)).not.toHaveClass(/\bcurrent\b/);
  await header.getByRole('button', { name: 'Next step' }).click();
  await expect(steps.nth(0)).toHaveClass(/\bcurrent\b/);
  await header.getByRole('button', { name: 'Previous step' }).click();
  await expect(steps.nth(2)).toHaveClass(/\bcurrent\b/);
  await blocks.nth(1).click();
  await expect(steps.nth(1)).toHaveClass(/\bcurrent\b/);
  await expect(page.getByRole('region', { name: 'Timeline' })).toContainText('Playing step 2 of 3');
  await header.getByRole('button', { name: 'Stop', exact: true }).click();
  await expect(steps.locator('li.current, .current')).toHaveCount(0);

  // Stopped, a step picked is shown on the lights when the preview is on.
  await page.getByRole('button', { name: 'DMX monitor' }).click();
  const cell = page.locator('.monitor .cell[title*="Wheel spot — Dimmer"] .v');
  await expect(cell).toHaveText('0');
  await page.getByLabel('Live preview on the lights').check();
  await steps.nth(0).locator('button.pick').click();
  await expect(steps.nth(0)).toHaveClass(/\bpicked\b/);
  await expect(cell).toHaveText('255');
  await steps.nth(1).locator('button.pick').click();
  await expect(cell).toHaveText('0');
  await page.getByLabel('Live preview on the lights').uncheck();
  await page.getByRole('button', { name: 'Close' }).click();

  // Delete is in the menu.
  await sceneMenu(page, 'Delete chaser');
  await expect(page.getByRole('treeitem', { name: /Tester/ })).toHaveCount(0);
});

test('a movement is drawn on its preview, shows where the lights reach and which head is which', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Stop all' }).click();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New movement', exact: true }).click();
  const name = page.getByLabel('Movement name');
  await expect(name).toHaveValue(/^Movement \d+$/);
  const field = (label: string) => page.locator('label.slider', { hasText: label }).locator('input[type=number]');

  // The moving heads of the group, numbered in group order, and their reach.
  await page.getByLabel('Movement group').selectOption({ label: 'Beams' });
  const heads = page.getByRole('list', { name: 'Heads in order' }).locator('li');
  await expect(heads).toHaveText(['1 Spot 540 1', '2 Spot 540 2']);
  await expect(page.getByText('The lights reach pan ±270°, tilt ±135°')).toBeVisible();
  await expect(page.locator('[data-reach]')).toHaveCount(1);

  // Dragging the center moves the shape; the engine gets it on release.
  const center = page.getByRole('button', { name: /^Center:/ });
  const box = (await center.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 80, box.y + box.height / 2, { steps: 5 });
  await page.mouse.up();
  await expect.poll(async () => Number(await field('Center pan').inputValue())).toBeGreaterThan(0);
  // The arrow keys move it too, one degree at a time.
  const panned = Number(await field('Center pan').inputValue());
  await center.focus();
  await center.press('ArrowRight');
  await expect(field('Center pan')).toHaveValue(String(panned + 1));
  // So do the edges, for the size.
  const width = Number(await field('Width').inputValue());
  await page.getByRole('button', { name: /^Width:/ }).press('Shift+ArrowRight');
  await expect(field('Width')).toHaveValue(String(width + 20));

  // Beyond what the spots reach, the shape is flattened: said, not hidden.
  await field('Center tilt').fill('120');
  await field('Center tilt').press('Enter');
  await field('Height').fill('60');
  await field('Height').press('Enter');
  await expect(page.getByRole('status')).toContainText('beyond what the lights reach');
  await field('Center tilt').fill('0');
  await field('Center tilt').press('Enter');
  await expect(page.getByRole('status')).toHaveCount(0);

  // Phase presets.
  await page.getByRole('group', { name: 'Phase presets' }).getByRole('button', { name: 'Even' }).click();
  await expect(field('Phase spread')).toHaveValue('360');

  // Mirrored heads draw a second, dashed outline; a relative shape is placed
  // by an offset; the run mode and the fades are kept.
  await page.getByLabel('Mirrored heads').selectOption({ label: 'Every second head' });
  await expect(page.locator('[data-mirrored]')).toHaveCount(1);
  await expect(heads).toHaveText(['1 Spot 540 1', /^2 Spot 540 2\s*⇄$/]);
  await page.getByLabel('Run mode').selectOption({ label: 'Ping-pong' });
  await page.getByLabel('Fade in', { exact: true }).selectOption({ label: '2 beats' });
  await page.getByLabel('Relative to the position of the scenes').check();
  await expect(page.locator('label.slider', { hasText: 'Offset pan' })).toBeVisible();
  await field('Rotation').fill('45');
  await field('Rotation').press('Enter');

  // Regression: renaming right after a change dropped it. Both in one task.
  await page.evaluate(() => {
    const w = [...document.querySelectorAll<HTMLLabelElement>('label.slider')].find((l) => l.textContent?.includes('Width'));
    const input = w?.querySelector<HTMLInputElement>('input[type=number]');
    if (input) {
      input.value = '200';
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.dispatchEvent(new Event('change', { bubbles: true }));
    }
    const n = document.querySelector<HTMLInputElement>('input[aria-label="Movement name"]');
    if (n) {
      n.value = 'Sweep';
      n.dispatchEvent(new Event('change', { bubbles: true }));
    }
  });
  await expect(page.getByRole('treeitem', { name: /Sweep/ })).toBeVisible();
  await page.getByRole('treeitem', { name: /^Matrix \d+/ }).first().click();
  await page.getByRole('treeitem', { name: /Sweep/ }).click();
  await expect(field('Width')).toHaveValue('200');
  await expect(field('Phase spread')).toHaveValue('360');
  await expect(field('Rotation')).toHaveValue('45');
  await expect(page.getByLabel('Run mode').locator('option:checked')).toHaveText('Ping-pong');
  await expect(page.getByLabel('Mirrored heads').locator('option:checked')).toHaveText('Every second head');
  await expect(page.getByLabel('Fade in', { exact: true }).locator('option:checked')).toHaveText('2 beats');
  await expect(page.getByLabel('Relative to the position of the scenes')).toBeChecked();

  // Started and stopped from its header.
  const header = page.locator('.editor .head');
  await header.getByRole('button', { name: 'Start', exact: true }).click();
  await expect.poll(async () => (await runtime(page)).state.active.length).toBe(1);
  await header.getByRole('button', { name: 'Stop', exact: true }).click();
  await expect.poll(async () => (await runtime(page)).state.active.length).toBe(0);

  // A group without pan or tilt is said so.
  await page.getByLabel('Movement group').selectOption({ label: 'Front' });
  await expect(page.getByText('None of the lights in this group can move')).toBeVisible();
});

test('a cue list is recorded from the programmer, edited in place and run from its header', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Stop all' }).click();
  const programmerSize = async () => (await runtime(page)).state.programmer.length as number;
  const clearProgrammer = async () => {
    await page.getByRole('button', { name: 'Simple Desk' }).click();
    await page.getByRole('button', { name: 'Selection & intentions' }).click();
    await page.getByRole('button', { name: 'Clear', exact: true }).click();
    await expect.poll(programmerSize).toBe(0);
  };
  const full = async (fixture: string) => {
    await page.getByRole('button', { name: 'Simple Desk' }).click();
    await page.getByRole('button', { name: fixture, exact: true }).click();
    await page.getByRole('button', { name: 'Full' }).click();
    await expect.poll(programmerSize).toBeGreaterThan(0);
  };
  await clearProgrammer();

  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New cue list', exact: true }).click();
  const name = page.getByLabel('Cue list name');
  await expect(name).toHaveValue(/^Cue list \d+$/);
  const listName = await name.inputValue();
  const open = async () => {
    await page.getByRole('button', { name: 'FX', exact: true }).click();
    await page.getByRole('treeitem', { name: new RegExp(`^${listName}`) }).click();
  };
  await expect(page.getByText('Set the lights in the Simple Desk, then')).toBeVisible();
  await expect(page.getByRole('button', { name: /^Record cue/ })).toBeDisabled();

  // Two cues, recorded one after the other.
  await full('Dimmer 1');
  await open();
  await page.getByRole('button', { name: /^Record cue/ }).click();
  const cues = page.getByRole('list').filter({ has: page.getByLabel('Go to cue 1') }).locator(':scope > li');
  await expect(cues).toHaveCount(1);
  await clearProgrammer();
  await full('Dimmer 2');
  await open();
  await page.getByRole('button', { name: /^Record cue/ }).click();
  await expect(cues).toHaveCount(2);
  const numbers = () => cues.getByLabel('Number').evaluateAll((els) => els.map((e) => (e as HTMLInputElement).value));
  await expect.poll(numbers).toEqual(['1', '2']);

  // GO from the header: the current and the next cue are shown.
  const transport = page.getByRole('group', { name: 'Transport' });
  await transport.getByRole('button', { name: 'GO' }).click();
  await expect(page.locator('[data-current]')).toHaveText('Cue 1 Cue 1');
  await expect(page.locator('[data-next]')).toHaveText('Cue 2 Cue 2');
  await expect.poll(async () => (await runtime(page)).state.active.find((a: { position: number | null }) => a.position !== null)?.position).toBe(0);

  // Recorded while cue 1 plays: it goes after it, numbered in between.
  await page.getByRole('button', { name: /^Record cue/ }).click();
  await expect.poll(numbers).toEqual(['1', '1.1', '2']);
  await expect(cues.nth(1).getByLabel('Name')).toHaveValue('Cue 1.1');

  // Times are typed in seconds, or beats; follow and move in black per cue.
  const second = cues.nth(2);
  await second.getByLabel('Fade in', { exact: true }).fill('0.5');
  await second.getByLabel('Fade in', { exact: true }).press('Enter');
  await expect(second.getByLabel('Fade in', { exact: true })).toHaveValue('0.5 s');
  await second.getByLabel('Delay').fill('2 beats');
  await second.getByLabel('Delay').press('Enter');
  await expect(second.getByLabel('Delay')).toHaveValue('2 beats');
  await second.getByLabel('Follow').selectOption('auto');
  await expect(second.getByLabel('Go on after')).toHaveValue('5 s');
  await second.getByLabel('Move in black').selectOption({ label: 'Off' });
  // Two quick edits of two cues are both kept.
  await cues.nth(0).getByLabel('Name').fill('Intro');
  await cues.nth(0).getByLabel('Name').press('Enter');
  await second.getByLabel('Name').fill('Outro');
  await second.getByLabel('Name').press('Enter');
  await open();
  await expect(cues.nth(0).getByLabel('Name')).toHaveValue('Intro');
  await expect(second.getByLabel('Name')).toHaveValue('Outro');
  await expect(second.getByLabel('Fade in', { exact: true })).toHaveValue('0.5 s');
  await expect(second.getByLabel('Move in black').locator('option:checked')).toHaveText('Off');

  // Values on demand; updated from the programmer (Dimmer 2 joins cue 1).
  await cues.nth(0).getByRole('button', { name: /^1 value/ }).click();
  await expect(page.getByRole('list', { name: 'Values of cue 1' })).toContainText('Intensity 100%');
  await page.getByRole('button', { name: 'Update cue 1 from the programmer' }).click();
  await expect(cues.nth(0).getByRole('button', { name: /^2 values/ })).toBeVisible();

  // Moved down, it is numbered between its new neighbours.
  await page.getByRole('button', { name: 'Move cue 1.1 down' }).click();
  await expect.poll(numbers).toEqual(['1', '2', '3']);
  await expect(cues.nth(2).getByLabel('Name')).toHaveValue('Cue 1.1');
  // Duplicated, removed, renumbered.
  await page.getByRole('button', { name: 'Duplicate cue 1', exact: true }).click();
  await expect.poll(numbers).toEqual(['1', '1.1', '2', '3']);
  await expect(cues.nth(1).getByLabel('Name')).toHaveValue('Intro copy');
  await page.getByRole('button', { name: 'Remove cue 3', exact: true }).click();
  await expect.poll(numbers).toEqual(['1', '1.1', '2']);
  await page.getByRole('button', { name: 'More actions' }).click();
  await page.getByRole('menuitem', { name: /^Renumber/ }).click();
  await expect.poll(numbers).toEqual(['1', '2', '3']);

  // Go to a cue, then Stop.
  await page.getByRole('button', { name: 'Go to cue 3' }).click();
  await expect(page.locator('[data-current]')).toHaveText('Cue 3 Outro');
  await expect(page.locator('[data-next]')).toHaveCount(0);
  await expect(transport.getByRole('button', { name: 'GO' })).toBeDisabled();
  await transport.getByRole('button', { name: 'Stop' }).click();
  await expect.poll(async () => (await runtime(page)).state.active.length).toBe(0);
  await clearProgrammer();
});

test('a matrix is edited with its own header and its new algorithms and modes', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Stop all' }).click();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New matrix effect', exact: true }).click();
  const name = page.getByLabel('Matrix name');
  await expect(name).toHaveValue(/^Matrix \d+$/);
  // Heads numbered in group order.
  await expect(page.locator('.matrix .cell').first()).toHaveText('1');

  const algorithm = page.getByRole('group', { name: 'Algorithm' });
  // The colors chosen stay when the pattern changes.
  await page.getByLabel('Color', { exact: true }).fill('#00ff00');
  await algorithm.getByRole('button', { name: 'Twinkle' }).click();
  await expect(page.getByLabel('Density')).toHaveValue('0.3');
  await expect(page.getByLabel('Background')).toBeVisible();
  await expect(page.getByLabel('Color', { exact: true })).toHaveValue('#00ff00');
  // No axis for twinkle; there is one for fill.
  await expect(page.getByRole('group', { name: 'Axis' })).toHaveCount(0);
  // A chase has a width, and can go out from the center.
  await algorithm.getByRole('button', { name: 'Chase' }).click();
  await expect(page.getByLabel('Color', { exact: true })).toHaveValue('#00ff00');
  await expect(page.getByLabel('Chase width')).toHaveValue('1');
  await page.getByLabel('Chase width').fill('3');
  await expect(page.locator('.level', { hasText: 'Width' })).toContainText('3 cells');
  const axis = page.getByRole('group', { name: 'Axis' });
  await axis.getByRole('button', { name: 'From the center' }).click();
  await expect(axis.getByRole('button', { name: 'From the center' })).toHaveAttribute('aria-pressed', 'true');
  // A rainbow has no colors and no dimmer: each rules the other out.
  await algorithm.getByRole('button', { name: 'Rainbow' }).click();
  await expect(page.getByLabel('Rainbows')).toHaveValue('1');
  await expect(page.getByLabel('Color', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('group', { name: 'Controls' }).getByRole('button', { name: 'Dimmer' })).toBeDisabled();
  await page.getByLabel('Rainbows').fill('2');
  await expect(page.locator('.level', { hasText: 'Rainbows' })).toContainText('2');
  await algorithm.getByRole('button', { name: 'Fill' }).click();
  await expect(page.getByRole('group', { name: 'Controls' }).getByRole('button', { name: 'Dimmer' })).toBeEnabled();
  await expect(page.getByLabel('Chase width')).toHaveCount(0);
  await expect(axis.getByRole('button', { name: 'From the center' })).toHaveAttribute('aria-pressed', 'true');
  // On the dimmer only: levels instead of colors, and why the colors stay.
  const controls = page.getByRole('group', { name: 'Controls' });
  await controls.getByRole('button', { name: 'Dimmer' }).click();
  await expect(page.locator('[data-dimmer-hint]')).toBeVisible();
  await expect(page.locator('input[type=color]')).toHaveCount(0);
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('1');
  await expect(page.getByLabel('Background level')).toHaveValue('0');
  await expect(algorithm.getByRole('button', { name: 'Rainbow' })).toBeDisabled();
  await page.getByLabel('Level', { exact: true }).fill('0.5');
  await expect(page.locator('.level', { hasText: 'Level' }).first()).toContainText('50%');
  await axis.getByRole('button', { name: 'Rows' }).click();
  await page.getByLabel('Run mode').selectOption({ label: 'Ping-pong' });
  await page.getByLabel('Fade out', { exact: true }).selectOption({ label: '1 beat' });

  // Regression: renaming right after a change dropped it. Both in one task.
  await page.evaluate(() => {
    const mode = document.querySelector<HTMLSelectElement>('select[aria-label="Run mode"]');
    if (mode) {
      mode.value = 'SingleShot';
      mode.dispatchEvent(new Event('change', { bubbles: true }));
    }
    const n = document.querySelector<HTMLInputElement>('input[aria-label="Matrix name"]');
    if (n) {
      n.value = 'Wipe';
      n.dispatchEvent(new Event('change', { bubbles: true }));
    }
  });
  await expect(page.getByRole('treeitem', { name: /Wipe/ })).toBeVisible();
  await page.getByRole('treeitem', { name: /^Movement \d+/ }).first().click();
  await page.getByRole('treeitem', { name: /Wipe/ }).click();
  await expect(page.getByLabel('Run mode').locator('option:checked')).toHaveText('Single shot');
  await expect(axis.getByRole('button', { name: 'Rows' })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByLabel('Fade out', { exact: true }).locator('option:checked')).toHaveText('1 beat');
  await expect(controls.getByRole('button', { name: 'Dimmer' })).toHaveClass(/active/);
  await expect(page.getByLabel('Level', { exact: true })).toHaveValue('0.5');

  // Started and stopped from its header (looping, so that it lasts).
  await page.getByLabel('Run mode').selectOption({ label: 'Loop' });
  const header = page.locator('.editor .head');
  await header.getByRole('button', { name: 'Start', exact: true }).click();
  await expect.poll(async () => (await runtime(page)).state.active.length).toBe(1);
  await header.getByRole('button', { name: 'Stop', exact: true }).click();
  await expect.poll(async () => (await runtime(page)).state.active.length).toBe(0);
  // Its menu: duplicate.
  await page.getByRole('button', { name: 'More actions' }).click();
  await page.getByRole('menuitem', { name: 'Duplicate' }).click();
  await expect(name).toHaveValue('Wipe copy');
});

test('the position limits of a moving head are set at the patch and shown by the movement editor', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Stop all' }).click();

  // A moving head has them; a dimmer does not.
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.locator('.tree li', { hasText: 'Dimmer 1' }).first().locator('button.fx').click();
  await expect(page.getByRole('heading', { name: 'Position limits' })).toHaveCount(0);
  await page.locator('.tree li', { hasText: 'Spot 540 1' }).first().locator('button.fx').click();
  await expect(page.getByRole('heading', { name: 'Position limits' })).toBeVisible();
  await expect(page.getByLabel('Tilt max')).toHaveCount(0);
  // Switched on, it starts as the range of the light.
  await page.getByLabel('Limit the position').check();
  await expect(page.getByLabel('Tilt max')).toHaveValue('135');
  await expect(page.getByLabel('Pan min')).toHaveValue('-270');
  await page.getByLabel('Tilt max').fill('30');
  await page.getByLabel('Tilt max').press('Enter');
  await page.getByLabel('Tilt min').fill('-20');
  await page.getByLabel('Tilt min').press('Enter');
  // What the engine kept: open the dimmer, then the spot again.
  await page.locator('.tree li', { hasText: 'Dimmer 1' }).first().locator('button.fx').click();
  await page.locator('.tree li', { hasText: 'Spot 540 1' }).first().locator('button.fx').click();
  await expect(page.getByLabel('Tilt max')).toHaveValue('30');
  await expect(page.getByLabel('Tilt min')).toHaveValue('-20');
  // A minimum above the maximum drags the maximum along instead of being refused.
  await page.getByLabel('Tilt min').fill('50');
  await page.getByLabel('Tilt min').press('Enter');
  await expect(page.getByLabel('Tilt max')).toHaveValue('50');
  await page.getByLabel('Tilt min').fill('-20');
  await page.getByLabel('Tilt min').press('Enter');
  await page.getByLabel('Tilt max').fill('30');
  await page.getByLabel('Tilt max').press('Enter');
  await expect(page.getByLabel('Tilt min')).toHaveValue('-20');

  // The movement editor says what the lights of the group reach: the tilt is
  // cut by the limits of that spot.
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New movement', exact: true }).click();
  await page.getByLabel('Movement group').selectOption({ label: 'Beams' });
  await expect(page.getByText(/tilt -20° to 30°/)).toBeVisible();

  // Switched off, the whole range again.
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.locator('.tree li', { hasText: 'Spot 540 1' }).first().locator('button.fx').click();
  await page.getByLabel('Limit the position').uncheck();
  await expect(page.getByLabel('Tilt max')).toHaveCount(0);
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('treeitem', { name: /^Movement \d+/ }).last().click();
  await expect(page.getByLabel('Movement group').locator('option:checked')).toHaveText('Beams');
  await expect(page.getByText(/tilt ±135°/)).toBeVisible();
});
