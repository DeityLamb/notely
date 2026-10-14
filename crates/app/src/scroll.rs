//! Scrolling: the browser's own in the browser (see `page`), and on Linux
//! desktop touchpad scrolling matched to Chromium's.
//!
//! egui scrolls only while scroll events arrive, by the distance they give.
//! macOS, Windows and browsers speed touchpad movement up and keep the page
//! gliding after the fingers lift; Linux does neither and leaves both to
//! each app. Chromium is what most desktop apps scroll like, so its numbers
//! are used here. They were taken from a recording of the same gesture in
//! both (`testdata/`), which the test below replays.

use eframe::egui::{Context, Event, MouseWheelUnit, RawInput, TouchPhase, Ui};

/// Whether the platform leaves touchpad speed and gliding to the app.
const ENABLED: bool = cfg!(all(unix, not(target_os = "macos")));

/// Points a mouse wheel notch scrolls, about what browsers and native lists
/// do. egui's own default is 40.
const WHEEL_STEP: f32 = 120.0;
/// How much further the page moves than the touchpad reports: Chromium on
/// Wayland scrolls 12 pixels per reported unit (measured on a 2x display).
const TOUCHPAD_SPEED: f32 = 12.0;
/// Fingers that rest this long before lifting start no glide; a pause
/// this long in the movement also starts a new gesture.
const REST: f64 = 0.1;
/// Seconds between a touchpad's reports, until enough have been seen to
/// measure it.
const USUAL_INTERVAL: f64 = 0.007;
/// Slower lift-offs than this (points per second) stop at once.
const MIN_START: f32 = 120.0;

// Chromium's fling (`PhysicsBasedFlingCurve`): the glide covers what steady
// slowing at `DECELERATION` would, but at most `MAX_PAGES` window heights,
// and lasts as long as slowing at `DURATION_DECELERATION` would, but at most
// `MAX_DURATION`. Its position follows a cubic bezier that starts at the
// lift-off speed and eases out to a stop.
/// Points per second squared (2.7559e-5 inch/ms² at 96 points an inch).
const DECELERATION: f32 = 2645.7;
const DURATION_DECELERATION: f32 = 2300.0;
const MAX_DURATION: f32 = 2.0;
const MAX_PAGES: f32 = 3.0;
/// The bezier's control points; the first is steepened to the start speed.
const P1_X: f32 = 0.2;
const P2: (f32, f32) = (0.55, 1.0);
// Chromium's `FlingBooster`: a flick that lands on a glide still going the
// same way at `BOOST_ABOVE` points per second or more is boosted. It starts
// with that glide's start speed added to its own, and its distance cap
// grows by the same ratio.
const BOOST_ABOVE: f32 = 350.0;
/// The flick has to follow the catch within this many seconds of movement,
/// and the fingers have to keep moving at this speed or more meanwhile.
const BOOST_PAUSE: f64 = 0.05;
const BOOST_SCROLL_ABOVE: f32 = 150.0;

/// Sets the mouse wheel step. Call once at start-up.
pub fn configure(ctx: &Context) {
    ctx.options_mut(|options| options.input_options.line_scroll_speed = WHEEL_STEP);
}

/// Speeds up touchpad scrolling in the input egui is about to process.
pub fn speed_up(input: &mut RawInput) {
    if !ENABLED {
        return;
    }
    let mut started = Vec::new();
    for (i, event) in input.events.iter_mut().enumerate() {
        if let Event::MouseWheel { unit: MouseWheelUnit::Point, delta, phase, .. } = event {
            *delta *= TOUCHPAD_SPEED;
            if *phase == TouchPhase::Start {
                started.push(i);
            }
        }
    }
    // egui takes a gesture's first event as its start only and drops the
    // movement it carries: follow it with the same movement as a move.
    for i in started.into_iter().rev() {
        if let Event::MouseWheel { unit, delta, modifiers, .. } = input.events[i].clone() {
            let moved = Event::MouseWheel { unit, delta, phase: TouchPhase::Move, modifiers };
            input.events.insert(i + 1, moved);
        }
    }
}

/// A glide in progress.
struct Glide {
    start: f64,
    /// Speed it started at, signed.
    speed: f32,
    /// Total distance (signed) and duration.
    distance: f32,
    duration: f32,
    /// First control point of the bezier.
    p1: (f32, f32),
    /// How far along the glide the page already is.
    done: f32,
}

impl Glide {
    /// The glide a lift-off at `speed` starts in a window `viewport` high.
    /// `boost` is how many times faster it is than the flick alone was.
    fn new(start: f64, speed: f32, viewport: f32, boost: f32) -> Self {
        let fast = speed.abs();
        let distance = (fast * fast / (2.0 * DECELERATION)).min(MAX_PAGES * viewport * boost);
        let duration = (fast / DURATION_DECELERATION).min(MAX_DURATION);
        // Start at the lift-off speed: the curve's first slope.
        let slope = fast * duration / distance;
        let p1 = if P1_X * slope <= 1.0 { (P1_X, P1_X * slope) } else { (1.0 / slope, 1.0) };
        Glide { start, speed, distance: distance * speed.signum(), duration, p1, done: 0.0 }
    }

    /// Where the page is `t` seconds into the glide.
    fn position(&self, t: f32) -> f32 {
        let x = (t / self.duration).clamp(0.0, 1.0);
        let bezier = |a: f32, b: f32, s: f32| 3.0 * (1.0 - s).powi(2) * s * a + 3.0 * (1.0 - s) * s * s * b + s.powi(3);
        // The curve gives time and position for a parameter; find the
        // parameter of this time by halving.
        let (mut low, mut high) = (0.0, 1.0);
        for _ in 0..24 {
            let mid = (low + high) / 2.0;
            if bezier(self.p1.0, P2.0, mid) < x { low = mid } else { high = mid }
        }
        self.distance * bezier(self.p1.1, P2.1, (low + high) / 2.0)
    }

    /// How fast the page is moving `t` seconds into the glide, unsigned.
    fn speed_at(&self, t: f32) -> f32 {
        const STEP: f32 = 0.004;
        (self.position(t + STEP) - self.position(t)).abs() / STEP
    }
}

#[derive(Default)]
pub struct Fling {
    /// The gesture in progress: when each of its frames came and how many
    /// touchpad reports it brought, and the movement of the last two reports.
    frames: Vec<(f64, u32)>,
    tail: [f32; 2],
    /// Time and reports of frames inside gestures, all told, which give the
    /// time between two reports. The reports themselves carry no time.
    report_time: f64,
    reports: u32,
    glide: Option<Glide>,
    /// Start speed of a fast glide the current gesture caught, to add to
    /// the gesture's own if it ends in a flick the same way, and whether
    /// the gesture has moved since the catch.
    carried: Option<f32>,
    moved_since_catch: bool,
    /// Time of the gesture's latest movement.
    last_move: f64,
}

impl Fling {
    /// Seconds between two touchpad reports.
    fn interval(&self) -> f64 {
        if self.reports >= 8 { self.report_time / f64::from(self.reports) } else { USUAL_INTERVAL }
    }

    /// Reads this frame's scroll input and returns how far to glide now.
    /// `viewport` is the height of the scrolled view.
    pub fn update(&mut self, ui: &Ui, viewport: f32) -> f32 {
        if !ENABLED {
            return 0.0;
        }
        let (now, touched) = ui.input(|i| {
            let touched = i.pointer.any_pressed() || i.events.iter().any(|e| matches!(e, Event::Key { pressed: true, .. }));
            (i.time, touched)
        });
        if touched {
            self.stop();
        }
        // A glide runs up to the moment new movement interrupts it.
        let before = self.glide(now);
        ui.input(|i| {
            for event in &i.events {
                let Event::MouseWheel { unit, delta, phase, .. } = event else { continue };
                match (unit, phase) {
                    // A start's movement follows again as a move (see `speed_up`).
                    (MouseWheelUnit::Point, TouchPhase::Start) => self.catch(now),
                    (MouseWheelUnit::Point, TouchPhase::Move) => self.moved(now, delta.y),
                    (MouseWheelUnit::Point, TouchPhase::End) => self.lifted(now, viewport),
                    // A wheel notch, or a cancelled gesture, ends the glide.
                    _ => self.stop(),
                }
            }
        });
        before + self.glide(now)
    }

    /// A touchpad report moved the page by `dy`. This also catches a
    /// gliding page.
    fn moved(&mut self, now: f64, dy: f32) {
        self.catch(now);
        // A pause, going the other way, or moving slowly gives the caught
        // speed up. The first report after the catch has no earlier one to
        // take a speed from.
        let slow = self.moved_since_catch && dy.abs() / (self.interval() as f32) < BOOST_SCROLL_ABOVE;
        let kept = self.carried.is_some_and(|speed| now - self.last_move <= BOOST_PAUSE && speed * dy > 0.0 && !slow);
        if !kept {
            self.carried = None;
        }
        self.moved_since_catch = true;
        if now - self.last_move > REST {
            self.frames.clear();
        }
        self.last_move = now;
        self.tail = [self.tail[1], dy];
        match self.frames.last_mut() {
            Some((at, reports)) if *at == now => *reports += 1,
            _ => {
                // The frame before this one is now known not to be the
                // gesture's last, which is cut short by the lift, and from
                // the third on the app is past waking up: such a frame
                // shows the touchpad's steady rate.
                if let [.., (before, _), (at, reports)] = self.frames[..] {
                    if self.frames.len() >= 3 {
                        self.report_time += at - before;
                        self.reports += reports;
                    }
                }
                self.frames.push((now, 1));
            }
        }
    }

    /// The fingers left the touchpad: keep going at the speed they had,
    /// which Chromium takes from the last two reports.
    fn lifted(&mut self, now: f64, viewport: f32) {
        let interval = self.interval();
        let reports: u32 = self.frames.iter().map(|(_, n)| n).sum();
        let moving = reports >= 2 && now - self.last_move <= REST;
        let speed = if moving { (self.tail[0] + self.tail[1]) / (2.0 * interval) as f32 } else { 0.0 };
        self.frames.clear();
        let carried = self.carried.take().filter(|c| c * speed > 0.0 && speed.abs() >= BOOST_ABOVE);
        let boosted = speed + carried.unwrap_or(0.0);
        // The lift is reported a frame after the last movement, and that is
        // when the fingers really left: the glide runs from there.
        self.glide = (speed.abs() >= MIN_START).then(|| Glide::new(self.last_move, boosted, viewport, boosted / speed));
    }

    /// Stops a glide because fingers touched down, remembering its start
    /// speed if it was still fast.
    fn catch(&mut self, now: f64) {
        if let Some(glide) = self.glide.take() {
            let fast = glide.speed_at((now - glide.start) as f32) >= BOOST_ABOVE;
            self.carried = fast.then_some(glide.speed);
            self.moved_since_catch = false;
            self.last_move = now;
        }
    }

    fn stop(&mut self) {
        self.glide = None;
        self.carried = None;
        self.frames.clear();
    }

    /// Distance the glide covers up to `now` since the last call.
    fn glide(&mut self, now: f64) -> f32 {
        let Some(glide) = &mut self.glide else { return 0.0 };
        let t = (now - glide.start) as f32;
        let step = glide.position(t) - glide.done;
        glide.done += step;
        if t >= glide.duration {
            self.glide = None;
        }
        step
    }
}

/// Scrolling in the browser is the browser's own: the page holds an empty
/// element as tall as the document, the canvas stays fixed over it, and the
/// document is drawn at the page's scroll position. That gives the
/// browser's speed, glide, wheel animation and scrollbar exactly, which
/// the app could only imitate from the wheel events.
#[cfg(target_arch = "wasm32")]
pub mod page {
    use std::cell::Cell;

    use eframe::egui::{Context, Event, RawInput};
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast as _;

    thread_local! {
        /// Height last given to the page, to leave the page alone otherwise.
        static HEIGHT: Cell<f32> = const { Cell::new(0.0) };
    }

    /// Redraws whenever the page scrolls. Call once at start-up.
    pub fn follow(ctx: &Context) {
        let Some(window) = web_sys::window() else { return };
        let ctx = ctx.clone();
        let redraw = Closure::<dyn Fn()>::new(move || ctx.request_repaint());
        let _ = window.add_event_listener_with_callback("scroll", redraw.as_ref().unchecked_ref());
        // The listener lives as long as the page.
        redraw.forget();
    }

    /// Keeps wheel events from scrolling the document inside the app: the
    /// page has already scrolled for them. Sideways movement is left in,
    /// the page has no use for it and unwrapped code blocks do.
    pub fn leave_wheel_to_page(input: &mut RawInput) {
        input.events.retain_mut(|event| match event {
            Event::MouseWheel { delta, .. } => {
                delta.y = 0.0;
                delta.x != 0.0
            }
            _ => true,
        });
    }

    /// How far the page is scrolled.
    pub fn offset() -> f32 {
        web_sys::window().and_then(|w| w.scroll_y().ok()).unwrap_or(0.0) as f32
    }

    /// After a frame: makes the page as tall as the document, and scrolls
    /// it when the app moved the document itself, as it does to keep the
    /// caret in view. `drawn_at` is the page offset the frame started from.
    pub fn sync(height: f32, drawn_at: f32, now_at: f32) {
        let Some(window) = web_sys::window() else { return };
        if HEIGHT.get() != height {
            HEIGHT.set(height);
            if let Some(spacer) = window.document().and_then(|d| d.get_element_by_id("height")) {
                let _ = spacer.set_attribute("style", &format!("height: {height}px"));
            }
        }
        if (now_at - drawn_at).abs() > 0.5 {
            window.scroll_to_with_x_and_y(0.0, f64::from(now_at));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fingers moving at `speed` points per second, then lifting.
    fn flick(speed: f32, viewport: f32) -> Fling {
        let mut fling = Fling::default();
        for frame in 0..20 {
            fling.moved(f64::from(frame) / 100.0, speed / 100.0);
        }
        fling.lifted(0.19, viewport);
        fling
    }

    /// Total distance and duration of a glide stepped every `dt` seconds.
    fn travel(mut fling: Fling, dt: f64) -> (f32, f64) {
        let (mut distance, mut now) = (0.0, 0.19);
        while fling.glide.is_some() {
            now += dt;
            distance += fling.glide(now);
        }
        (distance, now - 0.19)
    }

    #[test]
    fn a_gentle_flick_slows_down_steadily() {
        // 1500 points a second stops after 1500 / 2300 seconds, having
        // covered what steady slowing covers.
        let (distance, time) = travel(flick(1500.0, 800.0), 1.0 / 60.0);
        assert!((distance - 1500.0 * 1500.0 / (2.0 * DECELERATION)).abs() < 1.0, "{distance}");
        assert!((0.64..0.68).contains(&time), "{time}");
    }

    #[test]
    fn a_hard_flick_is_capped_at_three_windows_and_two_seconds() {
        let (distance, time) = travel(flick(-30_000.0, 700.0), 1.0 / 120.0);
        assert!((distance + 2100.0).abs() < 1.0, "{distance}");
        assert!((1.99..2.02).contains(&time), "{time}");
    }

    #[test]
    fn a_glide_starts_at_the_lift_off_speed() {
        let mut fling = flick(1500.0, 800.0);
        let first = fling.glide(0.19 + 0.001);
        assert!((first / 0.001 - 1500.0).abs() < 30.0, "{first}");
    }

    #[test]
    fn a_slow_or_paused_lift_does_not_glide() {
        assert!(flick(60.0, 800.0).glide.is_none());
        let mut paused = Fling::default();
        paused.moved(0.0, 30.0);
        paused.lifted(0.5, 800.0);
        assert!(paused.glide.is_none());
    }

    #[test]
    fn touching_again_catches_the_page() {
        let mut fling = flick(1500.0, 800.0);
        fling.glide(0.2);
        fling.moved(0.3, 0.0);
        assert_eq!(fling.glide(0.32), 0.0);
    }

    /// A flick at `speed`, then a catch 60 ms into its glide: `slow` reports
    /// of slow movement, then a second flick at the same speed. Returns the
    /// speed the second glide starts at.
    fn second_flick(speed: f32, slow: usize) -> f32 {
        let mut fling = flick(speed, 800.0);
        let mut now = 0.25;
        for report in 0..slow + 6 {
            let moving = if report < slow { 100.0 } else { speed };
            fling.moved(now, moving / 100.0);
            now += 0.01;
        }
        fling.lifted(now, 800.0);
        fling.glide.unwrap().speed
    }

    #[test]
    fn a_flick_onto_a_fast_glide_adds_its_speed() {
        assert!((second_flick(3000.0, 0) - 6000.0).abs() < 1.0);
        assert!((second_flick(-3000.0, 0) + 6000.0).abs() < 1.0);
    }

    #[test]
    fn moving_slowly_after_the_catch_gives_the_boost_up() {
        // One slow report is the first after the catch and is let through;
        // a second one is measured, at 100 points a second.
        assert!((second_flick(3000.0, 1) - 6000.0).abs() < 1.0);
        assert!((second_flick(3000.0, 2) - 3000.0).abs() < 1.0);
    }

    /// Replays recorded flicks through this module and compares the page's
    /// path with the one Chrome took for the same flicks. The two may differ
    /// by 40 px plus what the page moved in its fastest of the last three
    /// frames, three times over: Chrome's page trails its input by a frame
    /// or two, and during a flick input reaches the two in different
    /// batches. Returns the largest difference seen while the page moved
    /// less than 10 px a frame.
    fn replay(desktop: &str, browser: &str, viewport: f32) -> f64 {
        let field = |line: &str, n: usize| line.split(' ').nth(n).unwrap().to_owned();
        let number = |line: &str, n: usize| field(line, n).parse::<f64>().unwrap();
        let input: Vec<&str> = desktop.lines().filter(|l| l.starts_with("w ")).collect();
        let chrome = browser.lines().filter(|l| l.starts_with("f ")).map(|l| (number(l, 1), number(l, 2)));

        // The page's offset in this app at each of Chrome's frame times.
        let mut fling = Fling::default();
        let (mut offset, mut next, mut worst, mut before) = (0.0_f64, 0, 0.0_f64, 0.0_f64);
        let mut steps = [0.0_f64; 3];
        for (time, expected) in chrome {
            while next < input.len() && number(input[next], 1) <= time {
                let (at, dy) = (number(input[next], 1), number(input[next], 2) as f32 * TOUCHPAD_SPEED);
                offset -= f64::from(fling.glide(at));
                match field(input[next], 4).as_str() {
                    "End" => fling.lifted(at, viewport),
                    _ => {
                        fling.moved(at, dy);
                        offset -= f64::from(dy);
                    }
                }
                next += 1;
            }
            offset -= f64::from(fling.glide(time));
            let per_frame = (expected - before).abs();
            steps = [steps[1], steps[2], per_frame];
            let fastest = steps.iter().fold(0.0_f64, |a, &b| a.max(b));
            let (difference, allowed) = ((offset - expected).abs(), 40.0 + 3.0 * fastest);
            assert!(difference < allowed, "at {time:.3} s: {offset:.0} here, {expected:.0} in Chrome");
            if per_frame < 10.0 {
                worst = worst.max(difference);
            }
            before = expected;
        }
        println!("largest difference while the page is slow: {worst:.0} px of {before:.0}");
        worst
    }

    #[test]
    fn recorded_flicks_scroll_like_chrome() {
        // Two hard flicks 1.66 s apart; the first glide has all but stopped
        // when the second lands. Chrome's viewport was 737 px high.
        let desktop = include_str!("../testdata/plain-desktop.log");
        let browser = include_str!("../testdata/plain-browser.log");
        assert!(replay(desktop, browser, 737.0) < 25.0);
    }

    #[test]
    fn a_flick_onto_a_fast_glide_is_boosted_like_chrome() {
        // Two flicks 1.15 s apart; the second lands on a glide still moving
        // at about 470 px/s. Chrome was fullscreen: its first glide ran to
        // three times 887 px.
        let desktop = include_str!("../testdata/boost-desktop.log");
        let browser = include_str!("../testdata/boost-browser.log");
        assert!(replay(desktop, browser, 887.0) < 45.0);
    }
}
