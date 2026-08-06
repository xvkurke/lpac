#include "relay.h"

#include "main.h"

#include <euicc/es10b.h>
#include <euicc/es9p.h>
#include <euicc/tostr.h>

#include <cjson-ext/cJSON_ex.h>

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define RELAY_MAX_LINE_SIZE (64U * 1024U * 1024U)

struct relay_card_state {
    uint8_t *transaction_id;
    uint32_t transaction_id_len;
};

static void secure_clear(void *data, size_t length) {
    volatile unsigned char *cursor = data;
    while (length-- > 0) {
        *cursor++ = 0;
    }
}

static void relay_card_state_clear(struct relay_card_state *state) {
    if (state->transaction_id != NULL) {
        secure_clear(state->transaction_id, state->transaction_id_len);
        free(state->transaction_id);
    }
    memset(state, 0, sizeof(*state));
}

static char *read_line_dynamic(FILE *stream) {
    size_t capacity = 4096;
    size_t length = 0;
    char *buffer = malloc(capacity);
    if (buffer == NULL) {
        return NULL;
    }

    for (;;) {
        const int character = fgetc(stream);
        if (character == EOF) {
            if (length == 0) {
                free(buffer);
                return NULL;
            }
            break;
        }
        if (character == '\n') {
            break;
        }
        if (length + 1 >= capacity) {
            if (capacity >= RELAY_MAX_LINE_SIZE) {
                secure_clear(buffer, length);
                free(buffer);
                return NULL;
            }
            size_t next_capacity = capacity * 2;
            if (next_capacity > RELAY_MAX_LINE_SIZE) {
                next_capacity = RELAY_MAX_LINE_SIZE;
            }
            char *resized = realloc(buffer, next_capacity);
            if (resized == NULL) {
                secure_clear(buffer, length);
                free(buffer);
                return NULL;
            }
            buffer = resized;
            capacity = next_capacity;
        }
        buffer[length++] = (char)character;
    }

    if (length > 0 && buffer[length - 1] == '\r') {
        length--;
    }
    buffer[length] = '\0';
    return buffer;
}

static const char *json_string(const cJSON *object, const char *name) {
    const cJSON *item = cJSON_GetObjectItemCaseSensitive(object, name);
    if (!cJSON_IsString(item) || item->valuestring == NULL) {
        return NULL;
    }
    return item->valuestring;
}

static const cJSON *json_payload(const cJSON *request) {
    const cJSON *payload = cJSON_GetObjectItemCaseSensitive(request, "payload");
    return cJSON_IsObject(payload) ? payload : NULL;
}

static int print_response(const char *request_id, const char *operation, bool ok, cJSON *payload,
                          const char *error_code, const char *error_message) {
    int result = -1;
    cJSON *root = cJSON_CreateObject();
    if (root == NULL) {
        cJSON_Delete(payload);
        return -1;
    }

    cJSON_AddStringOrNullToObject(root, "requestId", request_id);
    cJSON_AddStringOrNullToObject(root, "operation", operation);
    cJSON_AddBoolToObject(root, "ok", ok);
    if (payload != NULL) {
        cJSON_AddItemToObject(root, "payload", payload);
        payload = NULL;
    }
    if (!ok) {
        cJSON *error = cJSON_CreateObject();
        if (error == NULL) {
            goto exit;
        }
        cJSON_AddStringOrNullToObject(error, "code", error_code);
        cJSON_AddStringOrNullToObject(error, "message", error_message);
        cJSON_AddStringToObject(error, "subjectCode", euicc_ctx.http.status.subjectCode);
        cJSON_AddStringToObject(error, "reasonCode", euicc_ctx.http.status.reasonCode);
        cJSON_AddStringToObject(error, "subjectIdentifier", euicc_ctx.http.status.subjectIdentifier);
        cJSON_AddItemToObject(root, "error", error);
    }

    char *serialized = cJSON_PrintUnformatted(root);
    if (serialized == NULL) {
        goto exit;
    }
    if (fputs(serialized, stdout) < 0 || fputc('\n', stdout) == EOF || fflush(stdout) != 0) {
        free(serialized);
        goto exit;
    }
    free(serialized);
    result = 0;

exit:
    cJSON_Delete(payload);
    cJSON_Delete(root);
    return result;
}

static int print_invalid_request(const char *request_id, const char *operation, const char *message) {
    return print_response(request_id, operation, false, NULL, "INVALID_REQUEST", message);
}

static int handle_card_init(const char *request_id, const char *operation) {
    char *euicc_challenge = NULL;
    char *euicc_info_1 = NULL;
    cJSON *payload = NULL;

    if (es10b_get_euicc_challenge_r(&euicc_ctx, &euicc_challenge) < 0
        || es10b_get_euicc_info_r(&euicc_ctx, &euicc_info_1) < 0) {
        free(euicc_challenge);
        free(euicc_info_1);
        return print_response(request_id, operation, false, NULL, "CARD_INIT_FAILED",
                              "Failed to read eUICC challenge or EUICCInfo1");
    }

    payload = cJSON_CreateObject();
    if (payload == NULL) {
        free(euicc_challenge);
        free(euicc_info_1);
        return print_response(request_id, operation, false, NULL, "OUT_OF_MEMORY", "Allocation failed");
    }
    cJSON_AddStringToObject(payload, "euiccChallenge", euicc_challenge);
    cJSON_AddStringToObject(payload, "euiccInfo1", euicc_info_1);
    free(euicc_challenge);
    free(euicc_info_1);
    return print_response(request_id, operation, true, payload, NULL, NULL);
}

static int handle_card_authenticate_server(const char *request_id, const char *operation, const cJSON *input,
                                           struct relay_card_state *state) {
    const char *server_signed_1 = json_string(input, "serverSigned1");
    const char *server_signature_1 = json_string(input, "serverSignature1");
    const char *euicc_ci_pkid = json_string(input, "euiccCiPKIdToBeUsed");
    const char *server_certificate = json_string(input, "serverCertificate");
    const char *matching_id = json_string(input, "matchingId");
    const char *imei = json_string(input, "imei");
    char *authenticate_server_response = NULL;
    uint8_t *transaction_id = NULL;
    uint32_t transaction_id_len = 0;

    if (server_signed_1 == NULL || server_signature_1 == NULL || euicc_ci_pkid == NULL ||
        server_certificate == NULL) {
        return print_invalid_request(request_id, operation, "Missing server authentication material");
    }

    struct es10b_authenticate_server_param parameters = {
        .b64_serverSigned1 = (char *)server_signed_1,
        .b64_serverSignature1 = (char *)server_signature_1,
        .b64_euiccCiPKIdToBeUsed = (char *)euicc_ci_pkid,
        .b64_serverCertificate = (char *)server_certificate,
    };
    struct es10b_authenticate_server_param_user user_parameters = {
        .matchingId = matching_id,
        .imei = imei,
    };

    if (es10b_authenticate_server_r(&euicc_ctx, &transaction_id, &transaction_id_len,
                                    &authenticate_server_response, &parameters, &user_parameters) < 0) {
        free(transaction_id);
        free(authenticate_server_response);
        return print_response(request_id, operation, false, NULL, "AUTHENTICATE_SERVER_FAILED",
                              "eUICC rejected the SM-DP+ authentication material");
    }

    relay_card_state_clear(state);
    state->transaction_id = transaction_id;
    state->transaction_id_len = transaction_id_len;

    cJSON *payload = cJSON_CreateObject();
    if (payload == NULL) {
        free(authenticate_server_response);
        return print_response(request_id, operation, false, NULL, "OUT_OF_MEMORY", "Allocation failed");
    }
    cJSON_AddStringToObject(payload, "authenticateServerResponse", authenticate_server_response);
    cJSON_AddNumberToObject(payload, "transactionIdLength", (double)transaction_id_len);
    free(authenticate_server_response);
    return print_response(request_id, operation, true, payload, NULL, NULL);
}

static int handle_card_prepare_download(const char *request_id, const char *operation, const cJSON *input) {
    const char *profile_metadata = json_string(input, "profileMetadata");
    const char *smdp_signed_2 = json_string(input, "smdpSigned2");
    const char *smdp_signature_2 = json_string(input, "smdpSignature2");
    const char *smdp_certificate = json_string(input, "smdpCertificate");
    const char *confirmation_code = json_string(input, "confirmationCode");
    char *prepare_download_response = NULL;

    if (profile_metadata == NULL || smdp_signed_2 == NULL || smdp_signature_2 == NULL ||
        smdp_certificate == NULL) {
        return print_invalid_request(request_id, operation, "Missing profile preparation material");
    }

    struct es10b_prepare_download_param parameters = {
        .b64_profileMetadata = (char *)profile_metadata,
        .b64_smdpSigned2 = (char *)smdp_signed_2,
        .b64_smdpSignature2 = (char *)smdp_signature_2,
        .b64_smdpCertificate = (char *)smdp_certificate,
    };
    struct es10b_prepare_download_param_user user_parameters = {
        .confirmationCode = confirmation_code,
    };

    if (es10b_prepare_download_r(&euicc_ctx, &prepare_download_response, &parameters, &user_parameters) < 0) {
        free(prepare_download_response);
        return print_response(request_id, operation, false, NULL, "PREPARE_DOWNLOAD_FAILED",
                              "eUICC failed to prepare the profile download");
    }

    cJSON *payload = cJSON_CreateObject();
    if (payload == NULL) {
        free(prepare_download_response);
        return print_response(request_id, operation, false, NULL, "OUT_OF_MEMORY", "Allocation failed");
    }
    cJSON_AddStringToObject(payload, "prepareDownloadResponse", prepare_download_response);
    free(prepare_download_response);
    return print_response(request_id, operation, true, payload, NULL, NULL);
}

static int handle_card_install_bpp(const char *request_id, const char *operation, const cJSON *input) {
    const char *bound_profile_package = json_string(input, "boundProfilePackage");
    if (bound_profile_package == NULL) {
        return print_invalid_request(request_id, operation, "Missing Bound Profile Package");
    }

    struct es10b_load_bound_profile_package_result result = {0};
    const int operation_result = es10b_load_bound_profile_package_r(&euicc_ctx, &result, bound_profile_package);

    cJSON *payload = cJSON_CreateObject();
    if (payload == NULL) {
        free(result.iccid);
        return print_response(request_id, operation, false, NULL, "OUT_OF_MEMORY", "Allocation failed");
    }
    cJSON_AddNumberToObject(payload, "sequenceNumber", (double)result.seqNumber);
    cJSON_AddStringOrNullToObject(payload, "iccid", result.iccid);
    cJSON_AddNumberToObject(payload, "bppCommandId", (double)result.bppCommandId);
    cJSON_AddStringToObject(payload, "bppCommand", euicc_bppcommandid2str(result.bppCommandId));
    cJSON_AddNumberToObject(payload, "errorReason", (double)result.errorReason);
    cJSON_AddStringToObject(payload, "errorReasonName", euicc_errorreason2str(result.errorReason));
    free(result.iccid);

    if (operation_result < 0) {
        return print_response(request_id, operation, false, payload, "LOAD_BPP_FAILED",
                              "eUICC failed while loading the Bound Profile Package");
    }
    return print_response(request_id, operation, true, payload, NULL, NULL);
}

static int handle_card_cancel(const char *request_id, const char *operation, const cJSON *input,
                              struct relay_card_state *state) {
    const cJSON *reason_item = cJSON_GetObjectItemCaseSensitive(input, "reason");
    const int reason = cJSON_IsNumber(reason_item) ? reason_item->valueint
                                                   : ES10B_CANCEL_SESSION_REASON_ENDUSERREJECTION;
    char *cancel_response = NULL;

    if (state->transaction_id == NULL || state->transaction_id_len == 0) {
        return print_invalid_request(request_id, operation, "No active eUICC transaction to cancel");
    }

    struct es10b_cancel_session_param parameters = {
        .transactionId = state->transaction_id,
        .transactionIdLen = (uint8_t)state->transaction_id_len,
        .reason = (enum es10b_cancel_session_reason)reason,
    };
    if (es10b_cancel_session_r(&euicc_ctx, &cancel_response, &parameters) < 0) {
        free(cancel_response);
        return print_response(request_id, operation, false, NULL, "CARD_CANCEL_FAILED",
                              "eUICC failed to cancel the active session");
    }

    cJSON *payload = cJSON_CreateObject();
    if (payload == NULL) {
        free(cancel_response);
        return print_response(request_id, operation, false, NULL, "OUT_OF_MEMORY", "Allocation failed");
    }
    cJSON_AddStringToObject(payload, "cancelSessionResponse", cancel_response);
    free(cancel_response);
    relay_card_state_clear(state);
    return print_response(request_id, operation, true, payload, NULL, NULL);
}

static int handle_server_initiate(const char *request_id, const char *operation, const cJSON *input) {
    const char *server_address = json_string(input, "serverAddress");
    const char *euicc_challenge = json_string(input, "euiccChallenge");
    const char *euicc_info_1 = json_string(input, "euiccInfo1");
    char *transaction_id = NULL;
    struct es10b_authenticate_server_param response = {0};

    if (server_address == NULL || euicc_challenge == NULL || euicc_info_1 == NULL) {
        return print_invalid_request(request_id, operation, "Missing server address, challenge, or EUICCInfo1");
    }

    if (es9p_initiate_authentication_r(&euicc_ctx, &transaction_id, &response, server_address, euicc_challenge,
                                       euicc_info_1) < 0) {
        free(transaction_id);
        es10b_authenticate_server_param_free(&response);
        return print_response(request_id, operation, false, NULL, "INITIATE_AUTHENTICATION_FAILED",
                              euicc_ctx.http.status.message);
    }

    cJSON *payload = cJSON_CreateObject();
    if (payload == NULL) {
        free(transaction_id);
        es10b_authenticate_server_param_free(&response);
        return print_response(request_id, operation, false, NULL, "OUT_OF_MEMORY", "Allocation failed");
    }
    cJSON_AddStringToObject(payload, "transactionId", transaction_id);
    cJSON_AddStringToObject(payload, "serverSigned1", response.b64_serverSigned1);
    cJSON_AddStringToObject(payload, "serverSignature1", response.b64_serverSignature1);
    cJSON_AddStringToObject(payload, "euiccCiPKIdToBeUsed", response.b64_euiccCiPKIdToBeUsed);
    cJSON_AddStringToObject(payload, "serverCertificate", response.b64_serverCertificate);
    free(transaction_id);
    es10b_authenticate_server_param_free(&response);
    return print_response(request_id, operation, true, payload, NULL, NULL);
}

static int handle_server_authenticate_client(const char *request_id, const char *operation, const cJSON *input) {
    const char *server_address = json_string(input, "serverAddress");
    const char *transaction_id = json_string(input, "transactionId");
    const char *authenticate_server_response = json_string(input, "authenticateServerResponse");
    struct es10b_prepare_download_param response = {0};

    if (server_address == NULL || transaction_id == NULL || authenticate_server_response == NULL) {
        return print_invalid_request(request_id, operation,
                                     "Missing server address, transaction ID, or eUICC authentication response");
    }

    if (es9p_authenticate_client_r(&euicc_ctx, &response, server_address, transaction_id,
                                   authenticate_server_response) < 0) {
        es10b_prepare_download_param_free(&response);
        return print_response(request_id, operation, false, NULL, "AUTHENTICATE_CLIENT_FAILED",
                              euicc_ctx.http.status.message);
    }

    cJSON *payload = cJSON_CreateObject();
    if (payload == NULL) {
        es10b_prepare_download_param_free(&response);
        return print_response(request_id, operation, false, NULL, "OUT_OF_MEMORY", "Allocation failed");
    }
    cJSON_AddStringToObject(payload, "profileMetadata", response.b64_profileMetadata);
    cJSON_AddStringToObject(payload, "smdpSigned2", response.b64_smdpSigned2);
    cJSON_AddStringToObject(payload, "smdpSignature2", response.b64_smdpSignature2);
    cJSON_AddStringToObject(payload, "smdpCertificate", response.b64_smdpCertificate);
    es10b_prepare_download_param_free(&response);
    return print_response(request_id, operation, true, payload, NULL, NULL);
}

static int handle_server_get_bpp(const char *request_id, const char *operation, const cJSON *input) {
    const char *server_address = json_string(input, "serverAddress");
    const char *transaction_id = json_string(input, "transactionId");
    const char *prepare_download_response = json_string(input, "prepareDownloadResponse");
    char *bound_profile_package = NULL;

    if (server_address == NULL || transaction_id == NULL || prepare_download_response == NULL) {
        return print_invalid_request(request_id, operation,
                                     "Missing server address, transaction ID, or prepare-download response");
    }

    if (es9p_get_bound_profile_package_r(&euicc_ctx, &bound_profile_package, server_address, transaction_id,
                                         prepare_download_response) < 0) {
        free(bound_profile_package);
        return print_response(request_id, operation, false, NULL, "GET_BPP_FAILED", euicc_ctx.http.status.message);
    }

    cJSON *payload = cJSON_CreateObject();
    if (payload == NULL) {
        free(bound_profile_package);
        return print_response(request_id, operation, false, NULL, "OUT_OF_MEMORY", "Allocation failed");
    }
    cJSON_AddStringToObject(payload, "boundProfilePackage", bound_profile_package);
    free(bound_profile_package);
    return print_response(request_id, operation, true, payload, NULL, NULL);
}

static int handle_server_cancel(const char *request_id, const char *operation, const cJSON *input) {
    const char *server_address = json_string(input, "serverAddress");
    const char *transaction_id = json_string(input, "transactionId");
    const char *cancel_session_response = json_string(input, "cancelSessionResponse");

    if (server_address == NULL || transaction_id == NULL || cancel_session_response == NULL) {
        return print_invalid_request(request_id, operation,
                                     "Missing server address, transaction ID, or cancel-session response");
    }

    if (es9p_cancel_session_r(&euicc_ctx, server_address, transaction_id, cancel_session_response) < 0) {
        return print_response(request_id, operation, false, NULL, "SERVER_CANCEL_FAILED",
                              euicc_ctx.http.status.message);
    }
    return print_response(request_id, operation, true, cJSON_CreateObject(), NULL, NULL);
}

static int run_agent(bool card_agent) {
    struct relay_card_state card_state = {0};

    if (card_agent && main_init_euicc() != 0) {
        return -1;
    }

    for (;;) {
        char *line = read_line_dynamic(stdin);
        if (line == NULL) {
            break;
        }
        if (line[0] == '\0') {
            free(line);
            continue;
        }

        cJSON *request = cJSON_Parse(line);
        secure_clear(line, strlen(line));
        free(line);
        if (request == NULL || !cJSON_IsObject(request)) {
            print_invalid_request(NULL, NULL, "Input must be one JSON object per line");
            cJSON_Delete(request);
            continue;
        }

        const char *request_id = json_string(request, "requestId");
        const char *operation = json_string(request, "operation");
        const cJSON *input = json_payload(request);
        if (operation == NULL || input == NULL) {
            print_invalid_request(request_id, operation, "Request requires operation and payload fields");
            cJSON_Delete(request);
            continue;
        }

        if (strcmp(operation, "shutdown") == 0) {
            print_response(request_id, operation, true, cJSON_CreateObject(), NULL, NULL);
            cJSON_Delete(request);
            break;
        }

        if (card_agent) {
            if (strcmp(operation, "card.init") == 0) {
                handle_card_init(request_id, operation);
            } else if (strcmp(operation, "card.authenticateServer") == 0) {
                handle_card_authenticate_server(request_id, operation, input, &card_state);
            } else if (strcmp(operation, "card.prepareDownload") == 0) {
                handle_card_prepare_download(request_id, operation, input);
            } else if (strcmp(operation, "card.installBpp") == 0) {
                handle_card_install_bpp(request_id, operation, input);
            } else if (strcmp(operation, "card.cancel") == 0) {
                handle_card_cancel(request_id, operation, input, &card_state);
            } else {
                print_invalid_request(request_id, operation, "Unsupported Card Agent operation");
            }
        } else if (strcmp(operation, "server.initiateAuthentication") == 0) {
            handle_server_initiate(request_id, operation, input);
        } else if (strcmp(operation, "server.authenticateClient") == 0) {
            handle_server_authenticate_client(request_id, operation, input);
        } else if (strcmp(operation, "server.getBpp") == 0) {
            handle_server_get_bpp(request_id, operation, input);
        } else if (strcmp(operation, "server.cancel") == 0) {
            handle_server_cancel(request_id, operation, input);
        } else {
            print_invalid_request(request_id, operation, "Unsupported Server Agent operation");
        }

        cJSON_Delete(request);
    }

    relay_card_state_clear(&card_state);
    return 0;
}

static int applet_main(const int argc, char **argv) {
    if (argc < 2) {
        fputs("Usage: lpac relay <card-agent|server-agent>\n", stderr);
        return -1;
    }
    if (strcmp(argv[1], "card-agent") == 0) {
        return run_agent(true);
    }
    if (strcmp(argv[1], "server-agent") == 0) {
        return run_agent(false);
    }
    fputs("Usage: lpac relay <card-agent|server-agent>\n", stderr);
    return -1;
}

struct applet_entry applet_relay = {
    .name = "relay",
    .main = applet_main,
};
