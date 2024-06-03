import * as wasm from "vector-sheepy";

// Setup webgl2 and wasm. Returns true if successful, throws on internal err, false on bad document!
function setup() {
    let canvas = document.getElementById("canv");
    if (canvas == null || !(canvas instanceof HTMLCanvasElement)) return false;

    let webgl2 = canvas.getContext("webgl2", {
        depth: false,
        stencil: false,
        alpha: true,
        desynchronized: false,
        antialias: true,
        premultipliedAlpha: true,
        preserveDrawingBuffer: false,
        failIfMajorPerformanceCaveat: true,

    });
    if (webgl2 == null || !(webgl2 instanceof WebGL2RenderingContext)) return false;
    // Throws if something goes wrong
    // Safety - runs synchronously and only once, `wasm.frame` cannot be called at the same time.
    wasm.setup(webgl2);
    // Continue on with setup!
    addEventListener("mousemove", (event) => {
        // todo: this is a stinky i think
        let rect = canvas.getBoundingClientRect();
        mousepos.x = event.pageX - rect.left - window.scrollX;
        mousepos.y = event.pageY - rect.top - window.scrollY;
    })
    addEventListener("touchmove", function (event) {
        // only care about first touch
        let rect = canvas.getBoundingClientRect();
        mousepos.x = event.changedTouches[0].pageX - rect.left - window.scrollX;
        mousepos.y = event.changedTouches[0].pageY - rect.top - window.scrollY;
    })
    // If observer not supported, simply skip it.
    if (!('IntersectionObserver' in window) ||
        !('IntersectionObserverEntry' in window) ||
        !('intersectionRatio' in window.IntersectionObserverEntry.prototype) ||
        !('isIntersecting' in window.IntersectionObserverEntry.prototype)
    ) {
        console.log("No observer.");
    } else {
        let observer = new IntersectionObserver(function (entries, _) {
            if (entries.length != 1) return;
            let canvas = entries[0];
            if (canvas.isIntersecting) {
                resume()
            } else {
                suspend()
            }
        }, {
            //Root explicitly left out, to use viewport
            // Only notify when transitioning to/from wholly invisible
            threshold: [0.0],
        });
        observer.observe(canvas);
    }

    return true;
}

var mousepos = { x: 0.0, y: 0.0 };

// In the case something fails, fallback to a static image
function fallback() {
    console.log("Falling back!");
    let sheep = document.getElementById("fallback_sheep");
    if (sheep == null) return;
    sheep.classList.remove("hidden");
}

// Is the animation hidden because it's offscreen?
var is_suspended = false;
// Did any event cause the animation to stop permanently?
var gl_is_shown = false;
// Did it die during initialization? (Used to modify page to explain what went wrong)
var was_unsupported = false;
// Suspend the animation when offscreen
function suspend() {
    console.log("Suspending")
    is_suspended = true;
}
// Bring the animation back from offscreen
function resume() {
    console.log("Returning")
    if (is_suspended && gl_is_shown) {
        is_suspended = false;
        frame()
    }
}
// Redraw
function frame() {
    if (is_suspended) return;
    try {
        // Safety - runs single-threaded, and strictly after `wasm.init` has returned
        wasm.frame(mousepos.x, mousepos.y);
        requestAnimationFrame(frame);
    } catch (err) {
        console.warn("Failed to animate:", err);
        fallback()
        gl_is_shown = false;
    }
}

// Call setup, display fallback on any errors :3
try {
    let sheep = document.getElementById("fallback_sheep");
    if (sheep != null) sheep.classList.add("hidden");

    if (gl_is_shown = setup()) {
        requestAnimationFrame(frame)
    } else {
        was_unsupported = true;
        fallback()
    }
} catch (err) {
    console.warn("Failed to init webgl2:", err);
    was_unsupported = true;
    fallback()
}
// Add disclaimer to the about animation section that it did, in fact, break.
if (was_unsupported) {
    let element = document.getElementById("animation-failed");
    if (element) {
        element.innerText = "(unfortunately, unsupported in your browser!)"
    }
}
// Populate Phone and Email sections (hopefully avoid scriptless
// spiders from picking up my contact info)
// base64 encoded then chunkwise reversed
let email = ["BAZ21haWwuY29t", "YXNwZW5wcmF0dD"];

{
    // Who named this function, why are they like this
    let parsed_email = atob(email.reverse().join(''));
    let email_link = document.getElementById("email");
    if (email_link) {
        email_link.href = `mailto:${parsed_email}`;
        email_link.innerText += `: ${parsed_email}`;
    }
}