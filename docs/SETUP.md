# Setting Up

To set up a third party email server, you will need their respective authentication. 

> [!WARNING]
> This documentation page is incomplete, please add to it by contributing. This setup
> is now only for developers and not users. Please note you have to clone the repo to
> actually make use of these instructions.

## Google OAuth

Store the following environment variables in a .env file located in the root of this project.

```text
GOOGLE_CLIENT_ID=...
GOOGLE_CLIENT_SECRET=...
```

See how you can get yours from [these docs](https://developers.google.com/identity/oauth2/web/guides/get-google-api-clientid).

## Microsoft Outlook

This feature is not yet implemented but on our TODO list.

## SMTP Server

Free from the shackles of big tech. We will add support for
this soon as well.
