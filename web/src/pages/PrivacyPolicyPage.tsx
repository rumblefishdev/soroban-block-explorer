import { Card, Stack, type SxProps, type Theme } from '@mui/material';
import {
  contentLinkSx,
  monoFontFamily,
} from '@rumblefish/soroban-block-explorer-ui';

import { PageHeader } from './detail/PageHeader.js';

// Plain semantic HTML styled from one place: the text reads as a document
// (and survives copy-paste) instead of fifty separately styled Typography nodes.
const proseSx: SxProps<Theme> = (theme) => ({
  p: { xs: 2.5, md: 5 },
  typography: 'bodyRegular',
  color: 'text.secondary',
  '& h2': {
    typography: 'heading6SemiBold',
    color: 'text.primary',
    mt: 5,
    mb: 2,
  },
  '& h3': { typography: 'bodyBold', color: 'text.primary', mt: 3, mb: 1 },
  '& p, & ul': { mt: 0, mb: 2 },
  '& ul': { pl: 3 },
  '& li + li': { mt: 0.5 },
  '& strong': { color: 'text.primary' },
  '& a': { color: 'inherit', ...contentLinkSx(theme) },
  '& code': { fontFamily: monoFontFamily, fontSize: '0.9em' },
  '& > :first-child': { mt: 0 },
  '& > :last-child': { mb: 0 },
});

const EMAIL = (
  <strong>
    <a href="mailto:hello@rumblefish.pl">hello@rumblefish.pl</a>
  </strong>
);

const COMPANY = (
  <p>
    <strong>Rumble Fish Poland sp. z o.o.</strong>
    <br />
    Filipa Eisenberga 11/3
    <br />
    31-523 Kraków, Poland
  </p>
);

/**
 * Soroban Scan's privacy policy (task 0577). The text is supplied by its
 * author and published verbatim: change the wording only from a new version
 * of that text, and move the "Last updated" date with it.
 */
export default function PrivacyPolicyPage() {
  return (
    <Stack spacing={3} sx={{ maxWidth: 880, mx: 'auto' }}>
      <PageHeader
        title="Privacy Policy"
        subtitle="Last updated: September 23, 2026"
      />
      <Card component="article" sx={proseSx}>
        <p>
          This Privacy Policy explains how personal data and other information
          are processed when you use <strong>Soroban Scan</strong> (the
          “Explorer”, “Website” or “Service”), a blockchain explorer for the
          Stellar network and Soroban smart contracts.
        </p>
        <p>
          We believe in collecting as little information as reasonably necessary
          to provide, secure and improve the Service.
        </p>

        <h2>1. Data Controller</h2>
        <p>
          The controller of personal data processed in connection with Soroban
          Scan is:
        </p>
        {COMPANY}
        <p>
          KRS: 0000696628
          <br />
          NIP: 6772425725
          <br />
          REGON: 368368380
        </p>
        <p>Email: {EMAIL}</p>
        <p>
          If you have questions concerning this Privacy Policy or the processing
          of your personal data, you may contact us using the details above.
        </p>

        <h2>2. Information We Process</h2>
        <p>
          Soroban Scan does not require users to create an account and does not
          provide forms through which users directly submit personal data to us.
        </p>
        <p>
          When you use the Explorer, certain information may nevertheless be
          processed automatically as described below.
        </p>

        <h3>2.1. Technical and usage data</h3>
        <p>
          When you access Soroban Scan, our infrastructure and the third-party
          services used by the Website may process technical information
          associated with your request or browser, such as:
        </p>
        <ul>
          <li>IP address;</li>
          <li>browser and device information;</li>
          <li>operating system;</li>
          <li>date and time of requests;</li>
          <li>requested pages or resources;</li>
          <li>referring information, where provided by the browser;</li>
          <li>
            information necessary to detect automated or malicious traffic; and
          </li>
          <li>information about how the Website is used.</li>
        </ul>
        <p>
          We process technical information where necessary to provide and secure
          the Service and, where applicable and subject to your consent, to
          understand how the Service is used.
        </p>

        <h3>2.2. Public Stellar blockchain data</h3>
        <p>
          Soroban Scan retrieves, indexes and displays information that is
          publicly available on the Stellar blockchain, including, among other
          things:
        </p>
        <ul>
          <li>Stellar account addresses and other blockchain identifiers;</li>
          <li>transactions;</li>
          <li>transaction operations and results;</li>
          <li>smart contract activity;</li>
          <li>token and asset information; and</li>
          <li>
            other information recorded on or derived from the public Stellar
            network.
          </li>
        </ul>
        <p>
          Blockchain identifiers are generally pseudonymous rather than directly
          identifying. However, an address, public key or transaction
          information may constitute personal data where it can be associated
          with an identified or identifiable natural person.
        </p>
        <p>
          Soroban Scan does not create the underlying blockchain records and
          cannot modify or delete data recorded on the Stellar blockchain. The
          Explorer provides an interface for accessing and presenting
          information that is already publicly available through the Stellar
          network.
        </p>
        <p>
          We process this information for the purpose of providing the
          blockchain explorer and enabling users to search, inspect and
          understand publicly available Stellar network activity.
        </p>
        <p>
          Where such information constitutes personal data for which we act as
          controller, we rely on our legitimate interests in operating and
          providing a public blockchain explorer and facilitating access to and
          understanding of public blockchain information, subject to the rights
          and interests of affected individuals.
        </p>

        <h2>3. Analytics</h2>
        <p>
          We use <strong>Google Analytics 4</strong>, currently under
          measurement ID <strong>G-DFMXSJQ9DR</strong>, to understand how
          visitors use Soroban Scan.
        </p>
        <p>
          Google Analytics is loaded through <strong>Google Tag Manager</strong>
          , currently container <strong>GTM-TBF2GP5S</strong>.
        </p>
        <p>
          Depending on your consent choices and configuration, Google Analytics
          may process information concerning your interaction with the Website,
          including information about your browser, device, approximate location
          and use of individual pages.
        </p>
        <p>
          For visitors in the European Union, Switzerland and the United
          Kingdom, Google states that Google Analytics does not log or store
          individual IP addresses. IP addresses may be used to derive
          approximate geographic information before being discarded.
        </p>
        <p>
          Where required by applicable law, analytics technologies are activated
          only after you provide consent. You may withdraw or change your
          consent through the cookie settings available on the Website.
        </p>

        <h2>4. Protection Against Bots – Cloudflare Turnstile</h2>
        <p>
          We use <strong>Cloudflare Turnstile</strong> to protect Soroban Scan
          and its API against bots, abuse and malicious automated traffic.
        </p>
        <p>
          Before the Website retrieves certain information from our API,
          Turnstile may evaluate technical signals from your browser in order to
          determine whether the request appears to originate from a legitimate
          user rather than automated traffic.
        </p>
        <p>
          Such signals may include your IP address, browser User-Agent, TLS
          characteristics and other information concerning the browser
          environment.
        </p>
        <p>
          After successful verification, a short-lived verification token is
          generated. In our implementation, the token used by the Explorer is
          retained only in the memory of the browser tab and is not stored by
          Soroban Scan in a persistent browser cookie.
        </p>
        <p>
          We use Turnstile because it is necessary for the security and reliable
          operation of the Service and our infrastructure.
        </p>
        <p>
          Cloudflare may process Turnstile information both as our service
          provider and, for certain purposes such as improving its bot-detection
          capabilities, as an independent controller in accordance with its own
          privacy documentation.
        </p>

        <h2>5. Browser Local Storage</h2>
        <p>
          Soroban Scan uses your browser's <strong>localStorage</strong> only to
          remember your selected display theme — currently either{' '}
          <strong>light mode or dark mode</strong>.
        </p>
        <p>
          This preference is stored locally on your device. We do not use this
          localStorage entry to identify you or track your activity across
          websites.
        </p>

        <h2>6. Third-Party Content and Direct Browser Connections</h2>
        <p>
          Some functionality of Soroban Scan causes your browser to connect
          directly to infrastructure operated by third parties. When this
          happens, the relevant third party will generally receive technical
          information required for the connection, including your IP address and
          browser information.
        </p>

        <h3>NFT images</h3>
        <p>
          NFTs displayed in the Explorer may reference images hosted outside
          Soroban Scan. These resources can be hosted on arbitrary third-party
          servers or through decentralised storage systems such as IPFS.
        </p>
        <p>
          When your browser retrieves such an image, it connects directly to the
          location specified by the NFT's creator or metadata. The operator of
          that resource may therefore receive your IP address and other
          information normally transmitted as part of a web request.
        </p>
        <p>
          Soroban Scan is not the operator of those external resources and does
          not determine their privacy practices.
        </p>
        <p>
          Soroban Scan is configured not to transmit the Soroban Scan page
          address as the referrer when requesting these external NFT resources.
        </p>

        <h3>Stellar federation addresses</h3>
        <p>
          If you search for a Stellar federation address, for example{' '}
          <code>name*domain.com</code>, your browser may communicate directly
          with the federation server operated for the relevant domain.
        </p>
        <p>
          As a result, the operator of that domain may receive your IP address,
          browser information and the federation request necessary to resolve
          the address.
        </p>
        <p>
          The processing of information by that third-party server is governed
          by the policies and practices of its operator.
        </p>

        <h2>7. Hosting and Infrastructure</h2>
        <p>
          Soroban Scan is hosted using{' '}
          <strong>Amazon Web Services (AWS)</strong> infrastructure in the{' '}
          <strong>eu-central-1 (Frankfurt, Germany)</strong> region.
        </p>
        <p>
          Our API is additionally protected and delivered through{' '}
          <strong>Cloudflare</strong> infrastructure.
        </p>
        <p>
          As a consequence, AWS and Cloudflare may process technical
          information, including IP addresses and request information, to the
          extent necessary to provide hosting, networking, security and related
          infrastructure services.
        </p>
        <p>
          We use service providers under appropriate contractual and
          data-protection arrangements where required by applicable law.
        </p>

        <h2>8. Cookies and Similar Technologies</h2>
        <p>
          Soroban Scan may use cookies and similar technologies for the purposes
          described in this Privacy Policy.
        </p>
        <p>
          <strong>Strictly necessary technologies</strong> may be used to
          provide and secure the Service.
        </p>
        <p>
          <strong>Analytics technologies</strong> associated with Google
          Analytics help us understand how visitors use Soroban Scan. Where
          required by applicable law, these technologies are used on the basis
          of your consent.
        </p>
        <p>
          <strong>Preference storage</strong> is currently limited to storing
          your light/dark theme preference in your browser's localStorage.
        </p>
        <p>
          Where cookie settings are made available on the Website, you can use
          them to manage your preferences concerning non-essential technologies.
        </p>

        <h2>9. Legal Bases for Processing</h2>
        <p>
          Where the GDPR or another law requiring a legal basis applies, we
          process personal data on one or more of the following grounds:
        </p>
        <ul>
          <li>
            <strong>your consent</strong>, in particular for non-essential
            analytics and tracking technologies where consent is required;
          </li>
          <li>
            <strong>our legitimate interests</strong>, including operating,
            maintaining and improving Soroban Scan, providing access to public
            blockchain information, preventing abuse and ensuring the security
            and reliability of the Service; and
          </li>
          <li>
            <strong>compliance with legal obligations</strong>, where processing
            is required by applicable law.
          </li>
        </ul>
        <p>
          Where processing is based on consent, you may withdraw that consent at
          any time. Withdrawal does not affect the lawfulness of processing
          carried out before withdrawal.
        </p>

        <h2>10. Recipients and Service Providers</h2>
        <p>
          Information may be processed by or disclosed to service providers that
          support the operation of Soroban Scan, including:
        </p>
        <ul>
          <li>
            <strong>Amazon Web Services (AWS)</strong> – hosting and
            infrastructure;
          </li>
          <li>
            <strong>Cloudflare</strong> – API infrastructure, security and
            Turnstile bot protection; and
          </li>
          <li>
            <strong>Google</strong> – Google Analytics 4 and Google Tag Manager.
          </li>
        </ul>
        <p>
          Information may also be disclosed where required by law, a court order
          or a legally binding request from a competent public authority.
        </p>
        <p>
          Third-party providers may process information in countries outside the
          European Economic Area. Where required, transfers of personal data are
          subject to mechanisms recognised under applicable data-protection law,
          such as adequacy decisions or Standard Contractual Clauses.
        </p>

        <h2>11. Data Retention</h2>
        <p>
          We retain personal data only for as long as necessary for the purposes
          for which it is processed, taking into account applicable legal
          requirements, security requirements and limitation periods.
        </p>
        <p>
          Retention periods applicable to information processed by third-party
          services, including Google Analytics, Cloudflare and AWS, may depend
          on the relevant service configuration and the provider's applicable
          policies.
        </p>
        <p>
          The theme preference stored in localStorage remains on your device
          until you remove it, clear your browser storage or the Website changes
          the relevant stored preference.
        </p>
        <p>
          The temporary Turnstile token used by the Explorer is kept only in the
          memory of the browser tab and is not persistently stored by Soroban
          Scan.
        </p>
        <p>
          Information recorded on the Stellar blockchain is maintained by the
          decentralised Stellar network. Soroban Scan does not control the
          blockchain itself and cannot alter or erase records from the Stellar
          ledger.
        </p>

        <h2>12. No User Accounts or Contact Forms</h2>
        <p>
          Soroban Scan does not currently provide user accounts or forms through
          which users submit personal information to us.
        </p>
        <p>
          The <strong>“Report a bug”</strong> functionality redirects users to
          GitHub. If you choose to submit an issue or otherwise interact with
          GitHub, your interaction takes place through GitHub and is subject to
          GitHub's own privacy terms and account settings.
        </p>

        <h2>13. Your Data Protection Rights</h2>
        <p>
          Depending on your location and applicable law, you may have rights
          concerning personal data that we process about you, including the
          right to:
        </p>
        <ul>
          <li>request access to your personal data;</li>
          <li>request correction of inaccurate personal data;</li>
          <li>request deletion of personal data;</li>
          <li>request restriction of processing;</li>
          <li>object to processing based on legitimate interests;</li>
          <li>
            withdraw consent at any time where processing is based on consent;
          </li>
          <li>
            receive certain personal data in a structured, commonly used and
            machine-readable format where the right to data portability applies;
            and
          </li>
          <li>
            lodge a complaint with a competent data-protection supervisory
            authority.
          </li>
        </ul>
        <p>
          In Poland, the competent supervisory authority is the{' '}
          <strong>
            President of the Personal Data Protection Office (Prezes Urzędu
            Ochrony Danych Osobowych – UODO)
          </strong>
          .
        </p>
        <p>Some rights may be subject to limitations under applicable law.</p>
        <p>
          In particular, Soroban Scan cannot modify or delete information from
          the Stellar blockchain itself. If information displayed by the
          Explorer originates directly from the public blockchain, exercising a
          data protection right against Soroban Scan does not enable us to alter
          the underlying Stellar ledger. We will nevertheless assess requests
          concerning processing carried out by Soroban Scan itself in accordance
          with applicable law.
        </p>
        <p>
          To exercise your rights concerning processing for which we are the
          controller, contact us at {EMAIL}.
        </p>

        <h2>14. Third-Party Websites</h2>
        <p>
          Soroban Scan may contain links to third-party websites or services,
          including GitHub and websites referenced through blockchain data.
        </p>
        <p>
          When you leave Soroban Scan or interact directly with a third-party
          service, that third party may process your information under its own
          terms and privacy policy. We do not control the privacy practices of
          third-party websites or services.
        </p>

        <h2>15. Security</h2>
        <p>
          We implement technical and organisational measures designed to protect
          information processed through Soroban Scan against unauthorised
          access, alteration, disclosure or destruction.
        </p>
        <p>
          No Internet-based service can, however, guarantee absolute security.
        </p>

        <h2>16. Changes to this Privacy Policy</h2>
        <p>
          We may update this Privacy Policy when Soroban Scan, the technologies
          used by the Service or applicable legal requirements change.
        </p>
        <p>
          The current version will be made available through Soroban Scan. The
          “Last updated” date at the beginning of this Privacy Policy indicates
          when it was most recently revised.
        </p>

        <h2>17. Contact</h2>
        <p>
          For questions concerning privacy or the processing of personal data in
          connection with Soroban Scan, contact:
        </p>
        {COMPANY}
        <p>Email: {EMAIL}</p>
      </Card>
    </Stack>
  );
}
