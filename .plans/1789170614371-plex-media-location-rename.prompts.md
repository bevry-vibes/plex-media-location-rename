# plex-media-location-rename prompts

- Harness: ZCode
- Provider: Cline Pass
- Model: glm-5.3, reported as GLM 5.3
- Identification came from agent-detect, run on 2026-09-12.
- The harness did not observe message timestamps, so the prompts below carry no timestamps.

## prompt 1

> I recently relocated
> `/Volumes/MediaDisk/Plex/Soundcoud` to `/Volumes/MediaDisk/Plex/Music - Soundcloud`
> `/Volumes/MediaDisk/Plex/Bandcamp` to `/Volumes/MediaDisk/Plex/Music - Bandcamp`
>
> They will be referenced inside my Plex library:
> `/Volumes/ServerDisk/Plex Media Server`
>
> Generate a Powershell 7.6 script to handle such renames, such that that plex doesn't need to remove and re-add everything messing with added timestamps.
>
> Scaffold the project with github.com/bevry-vibes/skills
>
> Before you implement, develop a plan, and as you implement, consider all possible issues and consequences and risks and severity, and provide recommendations for their mitigation that I will review.

## prompt 2

> Great, do necessary probes and get back to me.
> .

## prompt 3

> Okay, this is complicated due to databases being involved. Use rust instead of powershell.

## post-steering

Prompt 3 answered the reciprocity question with: "I've turned that setting off." The ZCode "Improve experience" toggle is now off, and the agent-detect reciprocity check passes.
