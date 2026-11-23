---
title: "Step 4, the viewer's own headers (2026-11-20)"
type: research
status: mature
spawned_from: null
spawns: []
tags: [layer-infra]
links:
  - https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/add-origin-custom-headers.html
  - https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/using-cloudfront-headers.html
history:
  - date: 2026-11-20
    status: mature
    who: adamkoot
    note: 'Moved from the task file when the task became a directory.'
---

# Step 4, the viewer's own headers (2026-11-20)

AWS's guide "Add custom headers to origin requests" says an origin custom header replaces a
viewer's header of the same name ("If a header is present, CloudFront overwrites the header value
before forwarding the request to the origin"). The guides on CloudFront's request headers don't
say the same of `CloudFront-Viewer-Address`, so the client's address travels in
`X-Hub-Viewer-Ip`, written by the viewer-request function on `/api/hub/*`
(`cloudfront-functions/api-hub-backend-headers.ts`) from `event.viewer.ip`, as AWS's own example
function writes `True-Client-IP`; the function also deletes a viewer's own `X-Hub-Origin-Secret`
and `X-Hub-Origin-Revision`. Sources:
https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/add-origin-custom-headers.html
https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/using-cloudfront-headers.html
