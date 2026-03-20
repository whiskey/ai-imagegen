# 16-Bit Pixel Art Character Animation Pipeline

## Step 1: Generate the Still Character in ComfyUI

Use the **Playground workflow** with **Pony Diffusion V6 XL**.

**Positive prompt:**
```
16-bit pixel art character sprite, human figure, SNES era style,
side view, standing pose, clean pixel edges, limited color palette,
retro game aesthetic, sharp pixels, full body, game asset,
score_9, score_8_up, score_7_up
```

**Negative prompt:**
```
blurry, smooth gradients, 3D render, photorealistic, anti-aliased,
watermark, text, deformed, extra limbs, score_4, score_3
```

**Settings:**
- Model: `ponyDiffusionV6XL_v6StartWithThisOne.safetensors`
- Resolution: 512x512
- Sampler: DPM++ 2M Karras
- Steps: 25
- CFG: 7.5

## Step 2: Downscale to Pixel Art Size

Add an **ImageScale** node between VAEDecode and SaveImage in ComfyUI.

- **Method:** `nearest-exact` (preserves hard pixel edges)
- **Width:** 64 or 128
- **Height:** 64 or 128
- **Crop:** disabled

## Step 3: Generate Walking Animation

Upload the still character image to a free-tier image-to-video service:

- [Kling AI](https://klingai.com) — 5 free videos/day
- [Hailuo AI](https://hailuoai.video) — free tier
- [Pika](https://pika.art) — free credits on signup
- [PixVerse](https://pixverse.ai) — free tier, character motion mode

**Video prompt:**
```
pixel art character walking cycle, side view, looping animation, retro 16-bit game style
```

## Step 4: Extract Sprite Frames

Use ffmpeg to pull frames from the video, skipping every Nth frame:

```bash
# Extract every 4th frame (adjust N for desired frame count)
ffmpeg -i walking_video.mp4 -vf "select=not(mod(n\,4))" -vsync vfr frames/frame_%03d.png
```

For a typical walk cycle, aim for 4-8 frames.
