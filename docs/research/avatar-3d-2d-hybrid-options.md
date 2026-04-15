# Embedded Avatar Research: 3D Area With 2D Image Masking

## Goal
Replace the old 2D rigging approach with something that still lets us use a 2D face image, but renders in an embedded 3D-capable area and looks more believable.

## Bottom line
There are three viable paths:

1. Face-mesh overlay on a 3D head or face shell
2. Full VRM avatar with a customized face texture/material pipeline
3. Live2D/Cubism as a higher-quality 2D fallback

The best path for this app is:
- short term: embedded WebGL canvas in the Tauri app using a face-shell mesh + 2D image texture projection
- medium term: move to a VRM-based head with expressions and look-at controls
- fallback if we want faster implementation: Live2D instead of fake mouth/brow overlays

## Why the old 2D rigging hit a wall
A flat PNG with hand-positioned mouth/brow controls does not provide:
- side-angle depth
- believable head motion
- stable eye/mouth deformation
- realistic lighting/shading
- natural expression blending

It can only fake motion in one view.

## Option 1: 3D face shell + 2D image as projected texture
### Concept
- Render a lightweight 3D face/head mesh in Three.js or Babylon.js inside the app.
- Use the uploaded 2D portrait as a texture or projected mask on the front face.
- Drive mouth, eye, brow, and head motion through blendshapes or bone transforms.
- Keep the rear/side head neutral, stylized, or procedurally shaded.

### Why this fits
- Keeps the user workflow close to the current app: upload a face image.
- Gives real 3D transforms and better motion.
- Lets us embed the avatar inside the main app window using WebGL.

### Technical reality
This is viable, but not from the image alone. A single portrait can texture the front of the face, but not fully define:
- side head topology
- ear shape
- hair volume
- hidden facial areas

So the realistic implementation is:
- use a standard face/head mesh
- map the image onto the frontal UV region
- use neutral geometry/materials elsewhere
- optionally inpaint or synthesize side textures later

### Supporting references
- MediaPipe Face Landmarker for Web outputs 3D face landmarks, blendshape scores, and transformation matrices suitable for real-time avatar/effects rendering:
  - https://ai.google.dev/edge/mediapipe/solutions/vision/face_landmarker/web_js
- Three.js is a practical WebGL embedding layer for custom meshes/materials/textures inside a web-based app shell:
  - https://threejs.org/docs/

### Recommendation level
High.
This is the best custom path if we want the uploaded image to remain central.

## Option 2: VRM avatar with custom face texture/material pipeline
### Concept
- Use a VRM avatar runtime in WebGL.
- Replace or augment the face/albedo materials with user-specific image-derived textures.
- Use built-in expression systems for mouth shapes, blinking, gaze, and emotion.

### Why this fits
- VRM gives us a real avatar system instead of inventing one.
- Expression and look-at support already exist.
- It is much easier to get believable motion from a working avatar format than from a hacked 2D overlay.

### Constraint
A VRM pipeline wants a real model. A single uploaded face image is not enough by itself to produce a good full avatar automatically. We would need either:
- a base avatar that accepts face-texture customization
- a face-fitting pipeline that transfers the portrait onto an existing head
- a later image-to-3D generation step

### Supporting references
- `@pixiv/three-vrm` exposes VRM runtime structures including expression management and look-at support:
  - https://pixiv.github.io/three-vrm/docs/classes/three-vrm.VRM.html
  - https://pixiv.github.io/three-vrm/docs/interfaces/types-vrmc-vrm-1.0.Expressions.html
- VRM ecosystem home:
  - https://vrm.dev/en/

### Recommendation level
High for medium-term architecture.
This is the strongest long-term option if we want the avatar to become a proper character system.

## Option 3: Live2D / Cubism
### Concept
- Author or generate a proper layered 2D avatar model.
- Render it in the app using the Cubism Web SDK.
- Drive mouth/eyes/expressions with model parameters.

### Why this fits
- Much better than the old flat overlay system.
- Purpose-built for anime/VTuber-style facial animation.
- Faster route to expressive motion than inventing a 3D system if we stay fundamentally 2D.

### Constraint
This is not 3D. It is a sophisticated 2D rig. It also usually expects authored model layers instead of one flat portrait image.

### Supporting references
- Official Cubism SDK tutorials for Web, including lip-sync and parameter control examples:
  - https://docs.live2d.com/en/cubism-sdk-tutorials/top/

### Recommendation level
Medium.
Good fallback if we want better animation quickly without solving the full 3D/image-mapping problem.

## Best practical architecture for this app
### Recommended implementation order
1. Embedded avatar panel remains a web-rendered canvas in the Tauri UI.
2. Replace the old rig with a Three.js face-shell renderer.
3. Use uploaded 2D image as the front-face texture/mask.
4. Drive expressions from:
   - TTS phoneme buckets
   - blink timer
   - simple emotion state
   - optional later face-landmark fitting
5. Move later to VRM if we want full character creation and richer expression.

### Why not jump straight to "photo on full 3D face"
Because one portrait is underconstrained. Without extra generation or authoring, it will look wrong from side angles and around the jaw/hairline. The safer path is:
- frontal realism
- stylized side/head treatment
- controlled expression system

## Embedded-window feasibility
Yes. This does not need a separate browser popup.
A WebGL canvas inside the app is the correct embedding model for:
- Three.js
- Babylon.js
- three-vrm
- Live2D Web SDK

Tauri can host that just like the rest of the frontend because it is still a web UI surface.

## Decision
If the goal is "more realistic than the old rigging, still usable with a 2D uploaded face image," choose:
- Phase 1: Three.js face shell with projected portrait texture
- Phase 2: optional MediaPipe-based landmark fitting
- Phase 3: VRM migration for full avatar system

If the goal becomes "ship expressive avatar faster," choose:
- Live2D/Cubism instead of more custom 2D hacks
