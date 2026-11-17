//! Bounded TCP SNMP/TcpExt parsing with stable cAdvisor labels.

use crate::model::AdvancedTcp;

/// Parse one Tcp or TcpExt group from paired Linux SNMP header/value rows.
/// Missing groups are empty; malformed or duplicate groups return None.
#[must_use]
pub fn parse_advanced_tcp(text: &str, protocol: &str) -> Option<AdvancedTcp> {
    let prefix = match protocol {
        "Tcp" => "Tcp:",
        "TcpExt" => "TcpExt:",
        _ => return None,
    };
    let mut stats = AdvancedTcp::default();
    let mut found = false;
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    while let Some(line) = lines.next() {
        let Some(header) = line.strip_prefix(prefix) else {
            continue;
        };
        if found {
            return None;
        }
        let mut values = lines.next()?.strip_prefix(prefix)?.split_whitespace();
        for field in header.split_whitespace() {
            if !field.starts_with(|character: char| character.is_ascii_alphabetic())
                || !field.bytes().all(|byte| byte.is_ascii_alphanumeric())
            {
                return None;
            }
            let value = values.next()?;
            if protocol == "Tcp" && field == "MaxConn" {
                let value = value.parse::<i64>().ok().filter(|value| *value >= -1)?;
                if stats.max_connections.replace(value).is_some() {
                    return None;
                }
            } else if let Some(label) = label(protocol, field)
                && stats.values.insert(label, value.parse().ok()?).is_some()
            {
                return None;
            }
        }
        if values.next().is_some() {
            return None;
        }
        found = true;
    }
    Some(stats)
}

fn label(protocol: &str, field: &str) -> Option<&'static str> {
    match protocol {
        "Tcp" => match field {
            "RtoAlgorithm" => Some("rtoalgorithm"),
            "RtoMin" => Some("rtomin"),
            "RtoMax" => Some("rtomax"),
            "ActiveOpens" => Some("activeopens"),
            "PassiveOpens" => Some("passiveopens"),
            "AttemptFails" => Some("attemptfails"),
            "EstabResets" => Some("estabresets"),
            "CurrEstab" => Some("currestab"),
            "InSegs" => Some("insegs"),
            "OutSegs" => Some("outsegs"),
            "RetransSegs" => Some("retranssegs"),
            "InErrs" => Some("inerrs"),
            "OutRsts" => Some("outrsts"),
            "InCsumErrors" => Some("incsumerrors"),
            _ => None,
        },
        "TcpExt" => match field {
            "EmbryonicRsts" => Some("embryonicrsts"),
            "SyncookiesSent" => Some("syncookiessent"),
            "SyncookiesRecv" => Some("syncookiesrecv"),
            "SyncookiesFailed" => Some("syncookiesfailed"),
            "PruneCalled" => Some("prunecalled"),
            "RcvPruned" => Some("rcvpruned"),
            "OfoPruned" => Some("ofopruned"),
            "OutOfWindowIcmps" => Some("outofwindowicmps"),
            "LockDroppedIcmps" => Some("lockdroppedicmps"),
            "TW" => Some("tw"),
            "TWRecycled" => Some("twrecycled"),
            "TWKilled" => Some("twkilled"),
            "TCPTimeWaitOverflow" => Some("tcptimewaitoverflow"),
            "TCPTimeouts" => Some("tcptimeouts"),
            "TCPSpuriousRTOs" => Some("tcpspuriousrtos"),
            "TCPLossProbes" => Some("tcplossprobes"),
            "TCPLossProbeRecovery" => Some("tcplossproberecovery"),
            "TCPRenoRecoveryFail" => Some("tcprenorecoveryfail"),
            "TCPSackRecoveryFail" => Some("tcpsackrecoveryfail"),
            "TCPRenoFailures" => Some("tcprenofailures"),
            "TCPSackFailures" => Some("tcpsackfailures"),
            "TCPLossFailures" => Some("tcplossfailures"),
            "DelayedACKs" => Some("delayedacks"),
            "DelayedACKLocked" => Some("delayedacklocked"),
            "DelayedACKLost" => Some("delayedacklost"),
            "ListenOverflows" => Some("listenoverflows"),
            "ListenDrops" => Some("listendrops"),
            "TCPHPHits" => Some("tcphphits"),
            "TCPPureAcks" => Some("tcppureacks"),
            "TCPHPAcks" => Some("tcphpacks"),
            "TCPRenoRecovery" => Some("tcprenorecovery"),
            "TCPSackRecovery" => Some("tcpsackrecovery"),
            "TCPSACKReneging" => Some("tcpsackreneging"),
            "TCPFACKReorder" => Some("tcpfackreorder"),
            "TCPSACKReorder" => Some("tcpsackreorder"),
            "TCPRenoReorder" => Some("tcprenoreorder"),
            "TCPTSReorder" => Some("tcptsreorder"),
            "TCPFullUndo" => Some("tcpfullundo"),
            "TCPPartialUndo" => Some("tcppartialundo"),
            "TCPDSACKUndo" => Some("tcpdsackundo"),
            "TCPLossUndo" => Some("tcplossundo"),
            "TCPFastRetrans" => Some("tcpfastretrans"),
            "TCPSlowStartRetrans" => Some("tcpslowstartretrans"),
            "TCPLostRetransmit" => Some("tcplostretransmit"),
            "TCPRetransFail" => Some("tcpretransfail"),
            "TCPRcvCollapsed" => Some("tcprcvcollapsed"),
            "TCPDSACKOldSent" => Some("tcpdsackoldsent"),
            "TCPDSACKOfoSent" => Some("tcpdsackofosent"),
            "TCPDSACKRecv" => Some("tcpdsackrecv"),
            "TCPDSACKOfoRecv" => Some("tcpdsackoforecv"),
            "TCPAbortOnData" => Some("tcpabortondata"),
            "TCPAbortOnClose" => Some("tcpabortonclose"),
            "TCPAbortOnMemory" => Some("tcpabortonmemory"),
            "TCPAbortOnTimeout" => Some("tcpabortontimeout"),
            "TCPAbortOnLinger" => Some("tcpabortonlinger"),
            "TCPAbortFailed" => Some("tcpabortfailed"),
            "TCPMemoryPressures" => Some("tcpmemorypressures"),
            "TCPMemoryPressuresChrono" => Some("tcpmemorypressureschrono"),
            "TCPSACKDiscard" => Some("tcpsackdiscard"),
            "TCPDSACKIgnoredOld" => Some("tcpdsackignoredold"),
            "TCPDSACKIgnoredNoUndo" => Some("tcpdsackignorednoundo"),
            "TCPMD5NotFound" => Some("tcpmd5notfound"),
            "TCPMD5Unexpected" => Some("tcpmd5unexpected"),
            "TCPMD5Failure" => Some("tcpmd5failure"),
            "TCPSackShifted" => Some("tcpsackshifted"),
            "TCPSackMerged" => Some("tcpsackmerged"),
            "TCPSackShiftFallback" => Some("tcpsackshiftfallback"),
            "TCPBacklogDrop" => Some("tcpbacklogdrop"),
            "PFMemallocDrop" => Some("pfmemallocdrop"),
            "TCPMinTTLDrop" => Some("tcpminttldrop"),
            "TCPDeferAcceptDrop" => Some("tcpdeferacceptdrop"),
            "IPReversePathFilter" => Some("ipreversepathfilter"),
            "TCPReqQFullDoCookies" => Some("tcpreqqfulldocookies"),
            "TCPReqQFullDrop" => Some("tcpreqqfulldrop"),
            "TCPFastOpenActive" => Some("tcpfastopenactive"),
            "TCPFastOpenActiveFail" => Some("tcpfastopenactivefail"),
            "TCPFastOpenPassive" => Some("tcpfastopenpassive"),
            "TCPFastOpenPassiveFail" => Some("tcpfastopenpassivefail"),
            "TCPFastOpenListenOverflow" => Some("tcpfastopenlistenoverflow"),
            "TCPFastOpenCookieReqd" => Some("tcpfastopencookiereqd"),
            "TCPSynRetrans" => Some("tcpsynretrans"),
            "TCPOrigDataSent" => Some("tcporigdatasent"),
            "PAWSActive" => Some("pawsactive"),
            "PAWSEstab" => Some("pawsestab"),
            _ => None,
        },
        _ => None,
    }
}
