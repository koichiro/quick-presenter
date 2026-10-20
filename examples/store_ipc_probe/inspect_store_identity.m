#import <Foundation/Foundation.h>
#import <Security/Security.h>
int main(int argc, const char **argv) {
    @autoreleasepool {
        if (argc != 2) return 2;
        NSURL *url = [NSURL fileURLWithPath:[NSString stringWithUTF8String:argv[1]]];
        SecStaticCodeRef code = NULL;
        OSStatus status = SecStaticCodeCreateWithPath((__bridge CFURLRef)url, kSecCSDefaultFlags, &code);
        if (status) return 3;
        SecRequirementRef requirement = NULL;
        status = SecRequirementCreateWithString(CFSTR("anchor apple generic and identifier \"app.quickpresenter.QuickPresenter\""), kSecCSDefaultFlags, &requirement);
        if (status) return 4;
        status = SecStaticCodeCheckValidity(code, kSecCSDefaultFlags, requirement);
        CFDictionaryRef raw = NULL;
        OSStatus infoStatus = SecCodeCopySigningInformation(code, kSecCSSigningInformation, &raw);
        if (infoStatus) return 5;
        NSDictionary *info = (__bridge NSDictionary *)raw;
        NSDictionary *ent = info[(__bridge NSString *)kSecCodeInfoEntitlementsDict];
        NSDictionary *summary = @{
            @"trusted_expected_identifier_status": @(status),
            @"identifier": info[(__bridge NSString *)kSecCodeInfoIdentifier] ?: [NSNull null],
            @"team": info[(__bridge NSString *)kSecCodeInfoTeamIdentifier] ?: [NSNull null],
            @"sandbox": ent[@"com.apple.security.app-sandbox"] ?: [NSNull null],
            @"groups": ent[@"com.apple.security.application-groups"] ?: @[],
            @"entitlement_dictionary_available": @(ent != nil)
        };
        NSData *data = [NSJSONSerialization dataWithJSONObject:summary options:0 error:nil];
        fwrite(data.bytes, 1, data.length, stdout); puts("");
        CFRelease(raw); CFRelease(requirement); CFRelease(code);
        return status == errSecSuccess ? 0 : 1;
    }
}
