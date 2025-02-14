//! Types that contain information about attestation report.
//! The implementation is based on Attestation Service API version 4.
//! <https://api.trustedservices.intel.com/documents/sgx-attestation-api-spec.pdf>

use crate::AttestationError;
use crate::EndorsedAttestationReport;

use std::convert::TryFrom;
use std::fmt;
use std::time::*;

use anyhow::{anyhow, bail, ensure, Error, Result};
use chrono::DateTime;
use serde_json::Value;
use uuid::Uuid;

type SignatureAlgorithms = &'static [&'static webpki::SignatureAlgorithm];
static SUPPORTED_SIG_ALGS: SignatureAlgorithms = &[
    &webpki::ECDSA_P256_SHA256,
    &webpki::ECDSA_P256_SHA384,
    &webpki::ECDSA_P384_SHA256,
    &webpki::ECDSA_P384_SHA384,
    &webpki::RSA_PSS_2048_8192_SHA256_LEGACY_KEY,
    &webpki::RSA_PSS_2048_8192_SHA384_LEGACY_KEY,
    &webpki::RSA_PSS_2048_8192_SHA512_LEGACY_KEY,
    &webpki::RSA_PKCS1_2048_8192_SHA256,
    &webpki::RSA_PKCS1_2048_8192_SHA384,
    &webpki::RSA_PKCS1_2048_8192_SHA512,
    &webpki::RSA_PKCS1_3072_8192_SHA384,
];

/// A report generated by an enclave that contains measurement, identity and
/// other data related to enclave.
///
/// # Note
///
/// Do not confuse `SgxEnclaveReport` with `AttestationReport`.
/// `SgxEnclaveReport` is generated by SGX hardware and endorsed by Quoting
/// Enclave through local attestation. The endorsed `SgxEnclaveReport` is an
/// `SgxQuote`. The quote is then sent to some attestation service (IAS or
/// DCAP-based AS). The endorsed `SgxQuote` is an attestation report signed by
/// attestation service's private key, a.k.a., `EndorsedAttestationReport`.
pub struct SgxEnclaveReport {
    /// Security version number of host system's CPU
    pub cpu_svn: [u8; 16],
    /// Misc select bits for the target enclave. Reserved for future function
    /// extension.
    pub misc_select: u32,
    /// Attributes of the enclave, for example, whether the enclave is running
    /// in debug mode.
    pub attributes: [u8; 16],
    /// Measurement value of the enclave. See
    /// [`EnclaveMeasurement`](../types/struct.EnclaveMeasurement.html)
    pub mr_enclave: [u8; 32],
    /// Measurement value of the public key that verified the enclave. See
    /// [`EnclaveMeasurement`](../types/struct.EnclaveMeasurement.html)
    pub mr_signer: [u8; 32],
    /// Product ID of the enclave
    pub isv_prod_id: u16,
    /// Security version number of the enclave
    pub isv_svn: u16,
    /// Set of data used for communication between enclave and target enclave
    pub report_data: [u8; 64],
}

impl std::fmt::Debug for SgxEnclaveReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "cpu_svn: {:?}", self.cpu_svn)?;
        writeln!(f, "misc_select: {:?}", self.misc_select)?;
        writeln!(f, "attributes: {:?}", self.attributes)?;
        writeln!(f, "mr_enclave: {:?}", self.mr_enclave)?;
        writeln!(f, "mr_signer: {:?}", self.mr_signer)?;
        writeln!(f, "isv_prod_id: {}", self.isv_prod_id)?;
        writeln!(f, "isv_svn: {}", self.isv_svn)?;
        writeln!(f, "report_data: {:?}", &self.report_data.to_vec())
    }
}

impl fmt::Display for SgxEnclaveReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "CPU version (hex): {}", hex::encode(self.cpu_svn))?;
        writeln!(f, "SSA Frame extended feature set: {}", self.misc_select)?;
        writeln!(
            f,
            "Attributes of the enclave (hex): {}",
            hex::encode(self.attributes)
        )?;
        writeln!(
            f,
            "Enclave measurement (hex): {}",
            hex::encode(self.mr_enclave)
        )?;
        writeln!(
            f,
            "Hash of the enclave singing key (hex): {}",
            hex::encode(self.mr_signer)
        )?;
        writeln!(f, "Enclave product ID: {}", self.isv_prod_id)?;
        writeln!(f, "Security version of the enclave: {}", self.isv_svn)?;
        writeln!(
            f,
            "The value of REPORT (hex): {}",
            hex::encode(self.report_data)
        )
    }
}

impl SgxEnclaveReport {
    /// Parse bytes of report into `SgxEnclaveReport`.
    pub fn parse_from<'a>(bytes: &'a [u8]) -> Result<Self> {
        let mut pos: usize = 0;
        let mut take = |n: usize| -> Result<&'a [u8]> {
            if n > 0 && bytes.len() >= pos + n {
                let ret = &bytes[pos..pos + n];
                pos += n;
                Ok(ret)
            } else {
                bail!("Quote parsing error.")
            }
        };

        // Start parsing report by bytes following specifications. Don't
        // transmute directly, since there may cause endianness issue.
        // off 48, size 16
        let cpu_svn = <[u8; 16]>::try_from(take(16)?)?;

        // off 64, size 4
        let misc_select = u32::from_le_bytes(<[u8; 4]>::try_from(take(4)?)?);

        // off 68, size 28
        let _reserved = take(28)?;

        // off 96, size 16
        let attributes = <[u8; 16]>::try_from(take(16)?)?;

        // off 112, size 32
        let mr_enclave = <[u8; 32]>::try_from(take(32)?)?;

        // off 144, size 32
        let _reserved = take(32)?;

        // off 176, size 32
        let mr_signer = <[u8; 32]>::try_from(take(32)?)?;

        // off 208, size 96
        let _reserved = take(96)?;

        // off 304, size 2
        let isv_prod_id = u16::from_le_bytes(<[u8; 2]>::try_from(take(2)?)?);

        // off 306, size 2
        let isv_svn = u16::from_le_bytes(<[u8; 2]>::try_from(take(2)?)?);

        // off 308, size 60
        let _reserved = take(60)?;

        // off 368, size 64
        let mut report_data = [0u8; 64];
        let _report_data = take(64)?;
        let mut _it = _report_data.iter();
        for i in report_data.iter_mut() {
            *i = *_it.next().ok_or_else(|| anyhow!("Quote parsing error."))?;
        }

        ensure!(pos == bytes.len(), "Quote parsing error.");

        Ok(SgxEnclaveReport {
            cpu_svn,
            misc_select,
            attributes,
            mr_enclave,
            mr_signer,
            isv_prod_id,
            isv_svn,
            report_data,
        })
    }
}

/// SGX Quote structure version
#[derive(Debug, PartialEq)]
pub enum SgxQuoteVersion {
    /// EPID quote version
    V1(SgxEpidQuoteSigType),
    /// EPID quote version
    V2(SgxEpidQuoteSigType),
    /// ECDSA quote version
    V3(SgxEcdsaQuoteAkType),
}

/// Intel EPID attestation signature type
#[derive(Debug, PartialEq)]
pub enum SgxEpidQuoteSigType {
    Unlinkable,
    Linkable,
}

/// ECDSA attestation key type
#[derive(Debug, PartialEq)]
pub enum SgxEcdsaQuoteAkType {
    /// ECDSA-256-with-P-256 curve
    P256_256,
    /// ECDSA-384-with-P-384 curve
    P384_384,
}

impl std::fmt::Display for SgxQuoteVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SgxQuoteVersion::V1(key_type) => write!(f, "Version 1, EPID {:?} signature", key_type),
            SgxQuoteVersion::V2(key_type) => write!(f, "Version 2, EPID {:?} signature", key_type),
            SgxQuoteVersion::V3(key_type) => {
                write!(f, "Version 3, ECDSA {:?} attestation key", key_type)
            }
        }
    }
}

/// SGX Quote status
#[derive(PartialEq, Debug)]
pub enum SgxQuoteStatus {
    /// EPID signature of the ISV enclave QUOTE was verified correctly and the
    /// TCB level of the SGX platform is up-to-date.
    OK,
    /// EPID signature of the ISV enclave QUOTE was invalid. The content of the
    /// QUOTE is not trustworthy.
    ///
    /// For DCAP, the signature over the application report is invalid.
    SignatureInvalid,
    /// The EPID group has been revoked. When this value is returned, the
    /// revocation Reason field of the Attestation Verification Report will
    /// contain revocation reason code for this EPID group as reported in the
    /// EPID Group CRL. The content of the QUOTE is not trustworthy.
    GroupRevoked,
    /// The EPID private key used to sign the QUOTE has been revoked by
    /// signature. The content of the QUOTE is not trustworthy.
    SignatureRevoked,
    /// The EPID private key used to sign the QUOTE has been directly revoked
    /// (not by signature). The content of the QUOTE is not trustworthy.
    ///
    /// For DCAP, the attestation key or platform has been revoked.
    KeyRevoked,
    /// SigRL version in ISV enclave QUOTE does not match the most recent
    /// version of the SigRL. In rare situations, after SP retrieved the SigRL
    /// from IAS and provided it to the platform, a newer version of the SigRL
    /// is madeavailable. As a result, the Attestation Verification Report will
    /// indicate SIGRL_VERSION_MISMATCH. SP can retrieve the most recent version
    /// of SigRL from the IAS and request the platform to perform remote
    /// attestation again with the most recent version of SigRL. If the platform
    /// keeps failing to provide a valid QUOTE matching with the most recent
    /// version of the SigRL, the content of the QUOTE is not trustworthy.
    SigrlVersionMismatch,
    /// The EPID signature of the ISV enclave QUOTE has been verified correctly,
    /// but the TCB level of SGX platform is outdated (for further details see
    /// Advisory IDs). The platform has not been identified as compromised and
    /// thus it is not revoked. It is up to the Service Provider to decide
    /// whether or not to trust the content of the QUOTE, andwhether or not to
    /// trust the platform performing the attestation to protect specific
    /// sensitive information.
    GroupOutOfDate,
    /// The EPID signature of the ISV enclave QUOTE has been verified correctly,
    /// but additional configuration of SGX platform may beneeded(for further
    /// details see Advisory IDs). The platform has not been identified as
    /// compromised and thus it is not revoked. It is up to the Service Provider
    /// to decide whether or not to trust the content of the QUOTE, and whether
    /// or not to trust the platform performing the attestation to protect
    /// specific sensitive information.
    ///
    /// For DCAP, The Quote verification passed and the platform is patched to
    /// the latest TCB level but additional configuration of the SGX
    /// platform may be needed.
    ConfigurationNeeded,
    /// The EPID signature of the ISV enclave QUOTE has been verified correctly
    /// but due to certain issues affecting the platform, additional SW
    /// Hardening in the attesting SGX enclaves may be needed.The relying party
    /// should evaluate the potential risk of an attack leveraging the relevant
    /// issues on the attesting enclave, and whether the attesting enclave
    /// employs adequate software hardening to mitigate the risk.
    SwHardeningNeeded,
    /// The EPID signature of the ISV enclave QUOTE has been verified correctly
    /// but additional configuration for the platform and SW Hardening in the
    /// attesting SGX enclaves may be needed. The platform has not been
    /// identified as compromised and thus it is not revoked. It is up to the
    /// Service Provider to decide whether or not to trust the content of the
    /// QUOTE. The relying party should also evaluate the potential risk of an
    /// attack leveraging the relevant issues on the attestation enclave, and
    /// whether the attesting enclave employs adequate software hardening to
    /// mitigate the risk.
    ConfigurationAndSwHardeningNeeded,
    /// DCAP specific quote status. The Quote is good but TCB level of the
    /// platform is out of date. The platform needs patching to be at the latest
    /// TCB level.
    OutOfDate,
    /// DCAP specific quote status. The Quote is good but the TCB level of the
    /// platform is out of date and additional configuration of the SGX Platform
    /// at its current patching level may be needed. The platform needs patching
    /// to be at the latest TCB level.
    OutOfDateConfigurationNeeded,
    /// DCAP specific quote status. The signature over the application report is
    /// invalid.
    InvalidSignature,
    #[cfg(dcap)]
    /// The Quote verification failed due to an error in one of the input. For dcap only.
    Unspecified,
    /// Other unknown bad status.
    UnknownBadStatus,
}

impl From<&str> for SgxQuoteStatus {
    /// Convert from str status from the report to enum.
    fn from(status: &str) -> Self {
        match status {
            "OK" => SgxQuoteStatus::OK,
            "SIGNATURE_INVALID" => SgxQuoteStatus::SignatureInvalid,
            "GROUP_REVOKED" => SgxQuoteStatus::GroupRevoked,
            "SIGNATURE_REVOKED" => SgxQuoteStatus::SignatureRevoked,
            "KEY_REVOKED" => SgxQuoteStatus::KeyRevoked,
            "SIGRL_VERSION_MISMATCH" => SgxQuoteStatus::SigrlVersionMismatch,
            "GROUP_OUT_OF_DATE" => SgxQuoteStatus::GroupOutOfDate,
            "OUT_OF_DATE" => SgxQuoteStatus::OutOfDate,
            "OUT_OF_DATE_CONFIGURATION_NEEDED" => SgxQuoteStatus::OutOfDateConfigurationNeeded,
            "CONFIGURATION_NEEDED" => SgxQuoteStatus::ConfigurationNeeded,
            "SW_HARDENING_NEEDED" => SgxQuoteStatus::SwHardeningNeeded,
            "CONFIGURATION_AND_SW_HARDENING_NEEDED" => {
                SgxQuoteStatus::ConfigurationAndSwHardeningNeeded
            }
            #[cfg(dcap)]
            "UNSPECIFIED" => SgxQuoteStatus::Unspecified,
            _ => SgxQuoteStatus::UnknownBadStatus,
        }
    }
}

/// An application that hosts an enclave can ask the enclave to produce a report
/// (`SgxEnclaveReport`) and then pass this report to a platform service
/// (Quoting Enclave) to produce a type of credential that reflects the enclave
/// and platform state. The quote can be passed to entities off the platform,
/// and verified using Intel EPID signature verification techniques.
pub struct SgxQuote {
    /// Version of the quote structure
    pub version: SgxQuoteVersion,
    /// ID of the Intel EPID group of the platform belongs to
    pub gid: u32,
    /// Security version number of Quoting Enclave
    pub isv_svn_qe: u16,
    /// Security version number of PCE
    pub isv_svn_pce: u16,
    /// Vendor ID of Quoting Enclave
    pub qe_vendor_id: Uuid,
    /// User data
    pub user_data: [u8; 20],
    /// Report generated by the enclave
    pub isv_enclave_report: SgxEnclaveReport,
}

impl std::fmt::Debug for SgxQuote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "version: {:?}", self.version)?;
        writeln!(f, "gid: {}", self.gid)?;
        writeln!(f, "isv_svn_qe: {}", self.isv_svn_qe)?;
        writeln!(f, "isv_svn_pce: {}", self.isv_svn_pce)?;
        writeln!(f, "qe_vendor_id: {}", self.qe_vendor_id)?;
        writeln!(f, "user_data: {:?}", &self.user_data)?;
        write!(f, "isv_enclave_report: \n{:?}", self.isv_enclave_report)
    }
}

impl fmt::Display for SgxQuote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Version and signature/key type: {}", self.version)?;
        writeln!(f, "GID or reserved: {}", self.gid)?;
        writeln!(f, "Security version of the QE: {}", self.isv_svn_qe)?;
        writeln!(f, "Security version of the PCE: {}", self.isv_svn_pce)?;
        writeln!(f, "ID of the QE vendor: {}", self.qe_vendor_id)?;
        writeln!(
            f,
            "Custom user-defined data (hex): {}",
            hex::encode(self.user_data)
        )?;
        write!(f, "{}", self.isv_enclave_report)
    }
}

impl SgxQuote {
    /// Parse from bytes to `SgxQuote`.
    pub fn parse_from<'a>(bytes: &'a [u8]) -> Result<Self> {
        let mut pos: usize = 0;
        let mut take = |n: usize| -> Result<&'a [u8]> {
            if n > 0 && bytes.len() >= pos + n {
                let ret = &bytes[pos..pos + n];
                pos += n;
                Ok(ret)
            } else {
                bail!("Quote parsing error.")
            }
        };

        // Parse by bytes according to specifications.
        // off 0, size 2 + 2
        let version = match u16::from_le_bytes(<[u8; 2]>::try_from(take(2)?)?) {
            1 => {
                let signature_type = match u16::from_le_bytes(<[u8; 2]>::try_from(take(2)?)?) {
                    0 => SgxEpidQuoteSigType::Unlinkable,
                    1 => SgxEpidQuoteSigType::Linkable,
                    _ => bail!("Quote parsing error."),
                };
                SgxQuoteVersion::V1(signature_type)
            }
            2 => {
                let signature_type = match u16::from_le_bytes(<[u8; 2]>::try_from(take(2)?)?) {
                    0 => SgxEpidQuoteSigType::Unlinkable,
                    1 => SgxEpidQuoteSigType::Linkable,
                    _ => bail!("Quote parsing error."),
                };
                SgxQuoteVersion::V2(signature_type)
            }
            3 => {
                let attestation_key_type = match u16::from_le_bytes(<[u8; 2]>::try_from(take(2)?)?)
                {
                    2 => SgxEcdsaQuoteAkType::P256_256,
                    3 => SgxEcdsaQuoteAkType::P384_384,
                    _ => bail!("Quote parsing error."),
                };
                SgxQuoteVersion::V3(attestation_key_type)
            }
            _ => bail!("Quote parsing error."),
        };

        // off 4, size 4
        let gid = u32::from_le_bytes(<[u8; 4]>::try_from(take(4)?)?);

        // off 8, size 2
        let isv_svn_qe = u16::from_le_bytes(<[u8; 2]>::try_from(take(2)?)?);

        // off 10, size 2
        let isv_svn_pce = u16::from_le_bytes(<[u8; 2]>::try_from(take(2)?)?);

        // off 12, size 16
        let qe_vendor_id_raw = <[u8; 16]>::try_from(take(16)?)?;
        let qe_vendor_id = Uuid::from_slice(&qe_vendor_id_raw)?;

        // off 28, size 20
        let user_data = <[u8; 20]>::try_from(take(20)?)?;

        // off 48, size 384
        let isv_enclave_report = SgxEnclaveReport::parse_from(take(384)?)?;

        ensure!(pos == bytes.len(), "Quote parsing error.");

        Ok(Self {
            version,
            gid,
            isv_svn_qe,
            isv_svn_pce,
            qe_vendor_id,
            user_data,
            isv_enclave_report,
        })
    }
}

/// A report that can be signed by Intel EPID (which generates
/// `EndorsedAttestationReport`) and then sent off of the platform to be
/// verified by remote client.
#[derive(Debug)]
pub struct AttestationReport {
    /// The freshness of the report, i.e., elapsed time after acquiring the
    /// report in seconds.
    pub freshness: Duration,
    /// Quote status
    pub sgx_quote_status: SgxQuoteStatus,
    /// Content of the quote
    pub sgx_quote_body: SgxQuote,
}

impl fmt::Display for AttestationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Report Freshness: {:?}", self.freshness)?;
        writeln!(f, "SGX Quote status: {:?}", self.sgx_quote_status)?;
        write!(f, "{}", self.sgx_quote_body)
    }
}

impl AttestationReport {
    /// Construct a AttestationReport from a X509 certificate and verify
    /// attestation report with the report_ca_cert which is from the attestation
    /// service provider.
    pub fn from_cert(certs: &[rustls::Certificate], report_ca_cert: &[u8]) -> Result<Self> {
        // Before we reach here, Webpki already verifed the cert is properly signed.
        use crate::cert::*;

        // Extract information for attestation from TLS certification.
        let x509 = yasna::parse_der(&certs[0].0, X509::load)?;
        let tbs_cert: <TbsCert as Asn1Ty>::ValueTy = x509.0;
        let pub_key: <PubKey as Asn1Ty>::ValueTy = ((((((tbs_cert.1).1).1).1).1).1).0;
        let pub_k = (pub_key.1).0;
        let cert_ext: <SgxRaCertExt as Asn1Ty>::ValueTy = (((((((tbs_cert.1).1).1).1).1).1).1).0;
        let cert_ext_payload: Vec<u8> = ((cert_ext.0).1).0;
        log::debug!("cert_ext_payload: {:?}", &cert_ext_payload);

        // Convert to endorsed report
        let report: EndorsedAttestationReport = serde_json::from_slice(&cert_ext_payload)?;
        log::debug!("endorsed attestation report: {:?}", &report);

        // Verify report's signature
        let signing_cert = webpki::EndEntityCert::try_from(report.certs[0].as_ref())?;

        let trust_anchors = vec![webpki::TrustAnchor::try_from_cert_der(report_ca_cert)?];
        let chain: Vec<&[u8]> = if report.certs.len() > 1 {
            let mut c = report.certs[1..]
                .iter()
                .map(|c| &c[..])
                .collect::<Vec<&[u8]>>();
            c.push(report_ca_cert);
            c
        } else {
            vec![report_ca_cert]
        };
        let time = webpki::Time::try_from(SystemTime::now())
            .map_err(|_| anyhow!("Cannot convert time."))?;
        signing_cert.verify_is_valid_tls_server_cert(
            SUPPORTED_SIG_ALGS,
            &webpki::TlsServerTrustAnchors(&trust_anchors),
            &chain,
            time,
        )?;

        // Verify the signature against the signing cert
        signing_cert.verify_signature(
            &webpki::RSA_PKCS1_2048_8192_SHA256,
            &report.report,
            &report.signature,
        )?;

        // Verify and extract information from attestation report
        let attn_report: Value = serde_json::from_slice(&report.report)?;
        log::trace!("attn_report: {}", attn_report);

        // Verify API version is supported
        let version = attn_report["version"]
            .as_u64()
            .ok_or_else(|| Error::new(AttestationError::ReportError))?;
        ensure!(version == 4, AttestationError::ApiVersionNotCompatible);

        // Get quote freshness
        let freshness = {
            let time = attn_report["timestamp"]
                .as_str()
                .ok_or_else(|| Error::new(AttestationError::ReportError))?;
            let time_fixed = String::from(time) + "+0000";
            let date_time = DateTime::parse_from_str(&time_fixed, "%Y-%m-%dT%H:%M:%S%.f%z")?;
            let ts = date_time.naive_utc();
            let now = DateTime::<chrono::offset::Utc>::from(SystemTime::now()).naive_utc();
            let quote_freshness = u64::try_from((now - ts).num_seconds())?;
            std::time::Duration::from_secs(quote_freshness)
        };

        // Get quote status
        let sgx_quote_status = {
            let status_string = attn_report["isvEnclaveQuoteStatus"]
                .as_str()
                .ok_or_else(|| Error::new(AttestationError::ReportError))?;
            SgxQuoteStatus::from(status_string)
        };

        // Get quote body
        let sgx_quote_body = {
            let quote_encoded = attn_report["isvEnclaveQuoteBody"]
                .as_str()
                .ok_or_else(|| Error::new(AttestationError::ReportError))?;
            let quote_raw = base64::decode(quote_encoded.as_bytes())?;
            SgxQuote::parse_from(quote_raw.as_slice())?
        };

        // According to RFC 5480 `Elliptic Curve Cryptography Subject Public Key
        // Information', SEC 2.2: ``The first octet of the OCTET STRING
        // indicates whether the key is compressed or uncompressed. The
        // uncompressed form is indicated by 0x04 and the compressed form is
        // indicated by either 0x02 or 0x03 (see 2.3.3 in [SEC1]). The public
        // key MUST be rejected if any other value is included in the first
        // octet.''
        //
        // We only accept the uncompressed form here.
        let raw_pub_k = pub_k.to_bytes();
        let is_uncompressed = raw_pub_k[0] == 4;
        let pub_k = &raw_pub_k.as_slice()[1..];
        if !is_uncompressed || pub_k != &sgx_quote_body.isv_enclave_report.report_data[..] {
            bail!(AttestationError::ReportError);
        }

        Ok(Self {
            freshness,
            sgx_quote_status,
            sgx_quote_body,
        })
    }
}

#[cfg(all(feature = "enclave_unit_test", feature = "mesalock_sgx"))]
pub mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Read;
    use std::untrusted::fs::File;

    fn tls_ra_cert_der_v3() -> Vec<u8> {
        let mut cert = vec![];
        let mut f = File::open("fixtures/tls_ra_cert_v3.der").unwrap();
        f.read_to_end(&mut cert).unwrap();

        cert
    }

    fn tls_ra_cert_der_v4() -> Vec<u8> {
        let mut cert = vec![];
        let mut f = File::open("fixtures/tls_ra_cert_v4.der").unwrap();
        f.read_to_end(&mut cert).unwrap();

        cert
    }

    fn ias_root_ca_cert_der() -> Vec<u8> {
        let mut cert = vec![];
        let mut f = File::open("fixtures/ias_root_ca_cert.der").unwrap();
        f.read_to_end(&mut cert).unwrap();

        cert
    }

    fn dcap_root_ca_cert_der() -> Vec<u8> {
        let mut cert = vec![];
        let mut f = File::open("fixtures/dcap_root_ca_cert.der").unwrap();
        f.read_to_end(&mut cert).unwrap();

        cert
    }

    fn attesation_report() -> Value {
        let report = json!({
            "version": 3,
            "timestamp": "2020-02-11T22:25:59.682915",
            "platformInfoBlob": "1502006504000900000D0D02040180030000000000000000000\
                                 A00000B000000020000000000000B2FE0AE0F7FD4D552BF7EF4\
                                 C938D44E349F1BD0E76F041362DC52B43B7B25994978D792137\
                                 90362F6DAE91797ACF5BD5072E45F9A60795D1FFB10140421D8\
                                 691FFD",
            "isvEnclaveQuoteStatus": "GROUP_OUT_OF_DATE",
            "isvEnclaveQuoteBody": "AgABAC8LAAAKAAkAAAAAAK1zRQOIpndiP4IhlnW2AkwAAAAA\
                                    AAAAAAAAAAAAAAAABQ4CBf+AAAAAAAAAAAAAAAAAAAAAAAAA\
                                    AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABwAAAAAAAAAHAAAA\
                                    AAAAADMKqRCjd2eA4gAmrj2sB68OWpMfhPH4MH27hZAvWGlT\
                                    AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACD1xnn\
                                    ferKFHD2uvYqTXdDA8iZ22kCD5xw7h38CMfOngAAAAAAAAAA\
                                    AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\
                                    AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\
                                    AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\
                                    AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\
                                    AAAAAAAAAADYIY9k0MVmCdIDUuFLf/2bGIHAfPjO9nvC7fgz\
                                    rQedeA3WW4dFeI6oe+RCLdV3XYD1n6lEZjITOzPPLWDxulGz",
            "id": "53530608302195762335736519878284384788",
            "epidPseudonym": "NRksaQej8R/SyyHpZXzQGNBXqfrzPy5KCxcmJrEjupXrq3xrm2y2+J\
                              p0IBVtcW15MCekYs9K3UH82fPyj6F5ciJoMsgEMEIvRR+csX9uyd54\
                              p+m+/RVyuGYhWbhUcpJigdI5Q3x04GG/A7EP10j/zypwqhYLQh0qN1\
                              ykYt1N1P0="
        });

        report
    }

    pub fn test_sgx_quote_parse_from() {
        let attn_report = attesation_report();
        let sgx_quote_body_encoded = attn_report["isvEnclaveQuoteBody"].as_str().unwrap();
        let quote_raw = base64::decode(sgx_quote_body_encoded.as_bytes()).unwrap();
        let sgx_quote = SgxQuote::parse_from(quote_raw.as_slice()).unwrap();

        assert_eq!(
            sgx_quote.version,
            SgxQuoteVersion::V2(SgxEpidQuoteSigType::Linkable)
        );
        assert_eq!(sgx_quote.gid, 2863);
        assert_eq!(sgx_quote.isv_svn_qe, 10);
        assert_eq!(sgx_quote.isv_svn_pce, 9);
        assert_eq!(
            sgx_quote.qe_vendor_id,
            Uuid::parse_str("00000000-ad73-4503-88a6-77623f822196").unwrap()
        );
        assert_eq!(
            sgx_quote.user_data,
            [117, 182, 2, 76, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );

        let isv_enclave_report = sgx_quote.isv_enclave_report;
        assert_eq!(
            isv_enclave_report.cpu_svn,
            [5, 14, 2, 5, 255, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(isv_enclave_report.misc_select, 0);
        assert_eq!(
            isv_enclave_report.attributes,
            [7, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            isv_enclave_report.mr_enclave,
            [
                51, 10, 169, 16, 163, 119, 103, 128, 226, 0, 38, 174, 61, 172, 7, 175, 14, 90, 147,
                31, 132, 241, 248, 48, 125, 187, 133, 144, 47, 88, 105, 83
            ]
        );
        assert_eq!(
            isv_enclave_report.mr_signer,
            [
                131, 215, 25, 231, 125, 234, 202, 20, 112, 246, 186, 246, 42, 77, 119, 67, 3, 200,
                153, 219, 105, 2, 15, 156, 112, 238, 29, 252, 8, 199, 206, 158
            ]
        );
        assert_eq!(isv_enclave_report.isv_prod_id, 0);
        assert_eq!(isv_enclave_report.isv_svn, 0);
        assert_eq!(
            isv_enclave_report.report_data.to_vec(),
            [
                216, 33, 143, 100, 208, 197, 102, 9, 210, 3, 82, 225, 75, 127, 253, 155, 24, 129,
                192, 124, 248, 206, 246, 123, 194, 237, 248, 51, 173, 7, 157, 120, 13, 214, 91,
                135, 69, 120, 142, 168, 123, 228, 66, 45, 213, 119, 93, 128, 245, 159, 169, 68,
                102, 50, 19, 59, 51, 207, 45, 96, 241, 186, 81, 179
            ]
            .to_vec()
        );
    }

    pub fn test_attestation_report_from_cert() {
        let tls_ra_cert = tls_ra_cert_der_v4();
        let dcap_root_ca_cert = dcap_root_ca_cert_der();
        let certs = vec![rustls::Certificate(tls_ra_cert)];
        let report = AttestationReport::from_cert(&certs, &dcap_root_ca_cert);
        assert!(report.is_ok());

        let report = report.unwrap();
        assert_eq!(report.sgx_quote_status, SgxQuoteStatus::OK);
    }

    pub fn test_attestation_report_from_cert_api_version_not_compatible() {
        let tls_ra_cert = tls_ra_cert_der_v3();
        let ias_root_ca_cert = ias_root_ca_cert_der();
        let certs = vec![rustls::Certificate(tls_ra_cert)];
        let report = AttestationReport::from_cert(&certs, &ias_root_ca_cert);
        assert!(report.is_err());
    }
}
